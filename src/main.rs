#![deny(clippy::all)]
#![windows_subsystem = "windows"]

use std::collections::BTreeSet;
use std::path::{PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::widgets::modrow::{Message as RowMessage, ModRow};
use humansize::{format_size, DECIMAL};
use iced::alignment::{Horizontal, Vertical};
use iced::event::Event;
use iced::widget::{button, column, container, horizontal_rule, horizontal_space, progress_bar, row, scrollable, text, vertical_rule, vertical_space, Space};
use iced::{event, time, window, Element, Length, Subscription, Task, Theme};
use iced::window::{icon};
use steamworks::{AppId, PublishedFileId};
use tokio::sync::oneshot;

use crate::presets::{Mod, ModPreset, PresetParser};
use crate::workshop::Workshop;

pub mod presets;
pub mod widgets;
pub mod workshop;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Amdu {
    // parser: Arc<Mutex<PresetParser>>,
    workshop: Option<Arc<Workshop>>,
    error: String,
    parser: PresetParser,
    mod_selection_list: Vec<ModRow>,
    workshop_subbed_mods: Vec<Mod>,
    toggle_all_state: bool,
    unsub_in_progress: bool,
    unsub_total_count: u32,
    unsub_progress: Arc<AtomicU32>,
}

#[derive(Debug, Clone)]
enum Message {
    EventOccurred(Event),
    OpenFileDialog,
    FilesPicked(Result<Arc<Vec<PathBuf>>, Error>),
    FilesParsed(Result<Arc<Vec<ModPreset>>, String>),
    List(usize, RowMessage),
    SubscribedModsFetched(Result<Arc<Vec<Mod>>, oneshot::error::RecvError>),
    LocalFileSizeFetched(Result<Arc<Vec<Mod>>, String>),
    Init(Result<(), String>),
    ToggleAll,
    UnsubSelected,
    UnsubProgress(Instant),
    UnsubbedSelectedMods(Result<(), String>),
    UpdateSelectionView(Arc<Vec<Mod>>),
}

impl Amdu {

    fn new() -> (Self, Task<Message>) {
        // let mut parser = Arc::new(Mutex::new(PresetParser::new()));

        let mut ws: Option<Arc<Workshop>> = None;
        let mut err: String = "".to_string();

        match Workshop::new(AppId(107410)) {
            Ok(result) => ws = Option::from(Arc::new(result)),
            Err(e) => err = e,
        }

        // print error if we get any
        if !err.is_empty() {
            println!("Workshop Error: {:?}", err);
        }

        (
            Self {
                workshop: ws,
                error: err,
                parser: PresetParser::new(),
                mod_selection_list: vec![],
                workshop_subbed_mods: vec![],
                toggle_all_state: true,
                unsub_in_progress: false,
                unsub_total_count: 0,
                unsub_progress: Arc::new(AtomicU32::new(0)),
            },
            Task::perform(init(), Message::Init),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            event::listen().map(Message::EventOccurred),
            // this is not the most optimal way to do it, but the polling ticker only runs during unsub progress... so should be fine
            match self.unsub_in_progress {
                false => Subscription::none(),
                true => time::every(Duration::from_millis(10)).map(Message::UnsubProgress),
            },
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EventOccurred(event) => {

                // if window close event, we drop workshop as this will trigger cleanup for the spawned thread there
                if let Event::Window(window::Event::CloseRequested) = event {
                    // stop workshop thread
                    self.workshop
                        .as_ref()
                        .unwrap()
                        .thread_shutdown_signal
                        .cancel();
                    // close window
                    window::get_latest().and_then(window::close)
                } else {
                    Task::none()
                }
            }
            Message::Init(Ok(_)) => {
                // init called as app is started
                // Don't fetch anything if workshop could not be initialized
                match self.workshop.clone() {
                    Some(ws) => {
                        Task::perform(load_subscribed_mods(ws), Message::SubscribedModsFetched)
                    }
                    None => Task::none(),
                }
            }
            Message::Init(Err(e)) => {
                println!("init: Error {:?}", e);
                Task::none()
            }
            Message::SubscribedModsFetched(result) => {
                match result {
                    Ok(mods) => {
                        self.workshop_subbed_mods = mods.to_vec();

                        Task::batch(vec![
                            Task::perform(
                                calculate_local_file_size(
                                    self.workshop_subbed_mods.clone(),
                                    self.workshop.clone().unwrap(),
                                ),
                                Message::LocalFileSizeFetched,
                            ),
                            Task::perform(
                                calculate_diff_mods(
                                    self.parser.get_modpresets(),
                                    self.workshop_subbed_mods.clone(),
                                ),
                                Message::UpdateSelectionView,
                            ),
                        ])
                    }
                    Err(_) => Task::none(),
                }
            }
            Message::UpdateSelectionView(diff_mods) => {
                // get diff, not calling as async as this is just straight vector diff and thus quick

                let mut mod_rows = vec![];
                for item in diff_mods.iter() {
                    let row = ModRow::new(
                        item.id,
                        item.name.clone(),
                        item.url.clone(),
                        item.local_filesize,
                        true,
                    );
                    mod_rows.push(row);
                }
                self.mod_selection_list = mod_rows;

                Task::none()
            }
            Message::LocalFileSizeFetched(result) => {
                match result {
                    Ok(mods) => {
                        self.workshop_subbed_mods = mods.to_vec();
                        // as we have updated data source now, update selection view by recalc
                        Task::perform(
                            calculate_diff_mods(
                                self.parser.get_modpresets(),
                                self.workshop_subbed_mods.clone(),
                            ),
                            Message::UpdateSelectionView,
                        )
                    }
                    Err(e) => {
                        println!("Failed fetching local install sizes with error: {:?}", e);
                        Task::none()
                    }
                }
            }
            Message::OpenFileDialog => {
                println!("opening file dialog btn pressed");
                Task::perform(pick_files(), Message::FilesPicked)
            }
            Message::FilesPicked(Ok(content)) => Task::perform(
                PresetParser::load_files_async(content.to_vec()),
                Message::FilesParsed,
            ),
            Message::FilesPicked(Err(error)) => {
                println!("Error on files picked: {:?}", error);

                Task::none()
            }
            Message::FilesParsed(Ok(data)) => {
                // save parsed to own state
                self.parser.set_modpresets(data.to_vec()).unwrap();

                Task::perform(
                    calculate_diff_mods(
                        self.parser.get_modpresets(),
                        self.workshop_subbed_mods.clone(),
                    ),
                    Message::UpdateSelectionView,
                )
            }
            Message::FilesParsed(Err(error)) => {
                println!("Error on files parsed: {:?}", error);
                Task::none()
            }
            Message::List(index, msg) => {
                match msg {
                    RowMessage::ToggleSelection(toggle) => {
                        self.mod_selection_list[index].selected = toggle;
                        Task::none()
                    }
                    RowMessage::ModPressed => {
                        // we do the same as toggle selection
                        self.mod_selection_list[index].selected =
                            !self.mod_selection_list[index].selected;

                        Task::none()
                    }
                }
            }
            Message::UnsubSelected => {
                self.unsub_in_progress = true;
                self.unsub_total_count = self
                    .mod_selection_list
                    .iter()
                    .filter(|item| item.selected)
                    .count() as u32;

                Task::perform(
                    unsub_selected_mods(
                        self.mod_selection_list.clone(),
                        self.workshop.clone().unwrap(),
                        self.unsub_progress.clone(),
                    ),
                    Message::UnsubbedSelectedMods,
                )
            }
            Message::UnsubbedSelectedMods(_) => {
                self.unsub_in_progress = false;
                Task::perform(
                    load_subscribed_mods(self.workshop.clone().unwrap()),
                    Message::SubscribedModsFetched,
                )
            }
            Message::UnsubProgress(_) => {
                // self.unsub_progress = *progress;
                // just ticking gui update...
                Task::none()
            }
            Message::ToggleAll => {
                // toggle state
                self.toggle_all_state = !self.toggle_all_state;

                // update selection
                for val in self.mod_selection_list.iter_mut() {
                    val.selected = self.toggle_all_state;
                }

                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // ERROR PAGE
        if self.workshop.is_none() {
            let content = column![
                text("Arma3 Mod Differential Unsubscriber")
                    .width(Length::Fill)
                    .size(40)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Top),
                text("(Does not include subscribed scenarios)")
                    .width(Length::Fill)
                    .size(15)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Top),
                horizontal_rule(38),
                row![text(format!("An Error Occured: {:?}", self.error))
                    .size(30)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)]
                .spacing(10)
                .height(Length::FillPortion(400)),
                row![
                    horizontal_space(),
                    text(format!("v{}", VERSION))
                        .align_y(Vertical::Bottom)
                        .align_x(Horizontal::Right)
                ]
            ]
            .spacing(5)
            .padding(20);

            return container(content)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }

        // MAIN CONTENT
        let load_presets = column![
            text("Load presets you wish to keep")
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top),
            Space::with_height(Length::Fixed(15.0)),
            button("Load Presets")
                .padding(10)
                .on_press(Message::OpenFileDialog),
        ]
            .padding([5, 5])
            .align_x(Horizontal::Center)
            .height(150);

        // get loaded presets name only
        let scrollable_presets = scrollable(
            self.parser.get_modpresets().iter().fold(
                column![]
                    .spacing(6)
                    .width(Length::Fill),
                |col, i| col.push(text(i.name.clone()).align_x(Horizontal::Center)),
            ),
        )
        .width(Length::Fill)
        .height(100);

        let presets_loaded = column![
            text("Loaded Presets")
                .width(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top),
            vertical_space(),
            horizontal_rule(2),
            vertical_space(),
            scrollable_presets,
        ]
            .padding([5, 5])
            .align_x(Horizontal::Center)
            .height(150)
            .width(Length::FillPortion(200));

        // stats
        let selected_mods_count = self
            .mod_selection_list
            .iter()
            .filter(|item| item.selected)
            .count();
        let subscribed_mods_local_size_sum: u64 = self
            .mod_selection_list
            .iter()
            .filter(|item| item.selected)
            .map(|item| item.file_size)
            .sum();

        let mods_stats = column![
            row![
                text("Mods subscribed to:").width(Length::FillPortion(5)),
                text(format!("{:}", self.workshop_subbed_mods.len()))
                    .align_x(Horizontal::Right),
            ]
            // .align_items(Alignment::Start)
            .spacing(30),
            row![
                text("Mods to be removed:").width(Length::FillPortion(5)),
                text(format!("{:}", selected_mods_count)).align_x(Horizontal::Right),
            ]
            .spacing(30),
            row![
                text("Space that will free up:").width(Length::FillPortion(5)),
                text(format_size(subscribed_mods_local_size_sum, DECIMAL))
                    .align_x(Horizontal::Right),
            ]
            .spacing(30),
        ]
        .padding([5, 5])
        .spacing(20)
        .height(150)
        .width(300);

        let mut unsub_button = button(
            row![text("Unsub Selected Mods")
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(Vertical::Center)
                .align_x(Horizontal::Center)],
        )
        .padding([5, 5])
        .width(150)
        .height(150);

        if !self.mod_selection_list.is_empty() {
            unsub_button = unsub_button.on_press(Message::UnsubSelected);
        }

        let selection_list =
            self.mod_selection_list
                .iter()
                .enumerate()
                .fold(column![].spacing(6), |col, (i, _)| {
                    col.push(
                        self.mod_selection_list[i]
                            .view()
                            .map(move |msg| Message::List(i, msg)),
                    )
                });

        let scrollable: Element<Message> = match self.unsub_in_progress {
            false => scrollable(selection_list)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            true => column![
                text(format!(
                    "Unsubbing mod {} out of {}...",
                    self.unsub_progress.load(Ordering::Relaxed),
                    self.unsub_total_count
                ))
                .size(30)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top),
                progress_bar(
                    0.0..=self.unsub_total_count as f32,
                    self.unsub_progress.load(Ordering::Relaxed) as f32
                )
            ]
            .spacing(5)
            .padding(10)
            .into(),
        };

        let bottom_bar = row![
            button("Toggle All")
                .padding(10)
                .on_press(Message::ToggleAll),
            horizontal_space(),
            text(format!("v{}", VERSION)).align_y(Vertical::Bottom)
        ]
        .padding(5);

        let content = column![
            text("Arma3 Mod Differential Unsubscriber")
                .width(Length::Fill)
                .size(40)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top),
            text("(Does not include subscribed scenarios)")
                .width(Length::Fill)
                .size(15)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top),
            horizontal_rule(38),
            row![
                load_presets,
                vertical_rule(2),
                presets_loaded,
                vertical_rule(2),
                mods_stats,
                vertical_rule(2),
                horizontal_space(),
                unsub_button,
            ]
            .spacing(8)
            // .align_items(Alignment::Center)
            .height(160),
            horizontal_rule(38),
            row![scrollable]
                .spacing(10)
                .height(Length::FillPortion(400)),
                // .align_items(Alignment::Center), TODO
            bottom_bar, //.align_items(Alignment::Start),
        ]
        .spacing(5)
        .padding(20);
        // .align_items(Alignment::Start);

        container(content)
            // .width(Length::Fill)
            // .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

#[derive(Debug, Clone)]
enum Error {
    DialogClosed,
}

async fn pick_files() -> Result<Arc<Vec<PathBuf>>, Error> {
    let paths = rfd::AsyncFileDialog::new()
        .add_filter("html", &["html"])
        .set_directory("/")
        .set_title("Pick Preset Files")
        .pick_files()
        .await
        .ok_or(Error::DialogClosed)?;

    let vector_paths = paths
        .iter()
        .map(|handle| handle.path().to_path_buf())
        .collect();
    Ok(Arc::new(vector_paths))
    // let arc: Arc<[Path]> = vector_paths.into()
}

async fn init() -> Result<(), String> {
    // run when created, for init code
    Ok(())
}

async fn load_subscribed_mods(
    workshop: Arc<Workshop>,
) -> Result<Arc<Vec<Mod>>, oneshot::error::RecvError> {
    let mods = workshop.get_subscribed_mods_info().await.unwrap();
    let mut formatted_mods: Vec<Mod> = mods
        .iter()
        .filter(|item| !item.tags.contains(&"Scenario".to_owned()) && !item.tags.contains(&"Composition".to_owned()))
        .map(|result| Mod {
            id: result.published_file_id.0,
            url: result.url.clone(),
            tags: result.tags.clone(),
            name: result.title.clone(),
            local_filesize: 0,
        })
        .collect();
    formatted_mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(Arc::new(formatted_mods))
}

async fn calculate_diff_mods(keep_sets: Vec<ModPreset>, all_mods: Vec<Mod>) -> Arc<Vec<Mod>>
where
    Mod: std::cmp::Ord,
{
    // if we have empty all_mods, we should return empty diff
    if keep_sets.is_empty() {
        return Arc::new(all_mods);
    }

    let combined_keep_mods: Vec<_> = keep_sets
        .iter()
        .flat_map(|item| item.mods.clone())
        .collect();
    let keep_mods_set = BTreeSet::from_iter(combined_keep_mods);

    let mut diff_mods: Vec<Mod> = all_mods.clone();
    diff_mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    diff_mods.retain(|e| !keep_mods_set.contains(e));

    // sleep we need due to bug on windows causing some batch commands not run if return too fast: https://github.com/iced-rs/iced/issues/436
    tokio::time::sleep(Duration::from_millis(2)).await;

    Arc::new(diff_mods)
}

async fn calculate_local_file_size(
    mut mods: Vec<Mod>,
    workshop: Arc<Workshop>,
) -> Result<Arc<Vec<Mod>>, String> {
    // loop through vectors and ask per mod
    for val in mods.iter_mut() {
        match workshop.get_item_install_info(PublishedFileId(val.id)) {
            Some(result) => {
                val.local_filesize = result.size_on_disk;
            }
            None => {
                println!(
                    "Could not find mod locally installed with id: {:?}",
                    val.name
                )
            }
        }
    }
    // sleep we need due to bug on windows causing some batch commands not run if return too fast: https://github.com/iced-rs/iced/issues/436
    tokio::time::sleep(Duration::from_millis(2)).await;

    Ok(Arc::new(mods))
}

async fn unsub_selected_mods(
    mods: Vec<ModRow>,
    workshop: Arc<Workshop>,
    progress: Arc<AtomicU32>,
) -> Result<(), String> {
    for val in mods.iter().filter(|item| item.selected) {
        // for every loop we add one to progress to show what mod we are currently unsubbing
        progress.fetch_add(1, Ordering::Relaxed);

        // await unsub
        if workshop.unsub_from_mod(PublishedFileId(val.id)).await.is_ok() {}
    }
    Ok(())
}

pub fn main() -> iced::Result {
    iced::application("AMDU", Amdu::update, Amdu::view)
        .subscription(Amdu::subscription)
        .theme(Amdu::theme)
        .window(
            window::Settings {
                exit_on_close_request: false,
                icon: Some(icon::from_file_data(include_bytes!("../gfx/icon.png"), None).expect("Failed to load icon")),
                ..Default::default()
            })
        .run_with(Amdu::new)
}
