#![deny(clippy::all)]
#![windows_subsystem = "windows"]

use std::collections::BTreeSet;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::widgets::modrow::{Message as RowMessage, ModRow};
use iced::alignment::{Horizontal, Vertical};
use iced::event::{self, Event};
use iced::futures::StreamExt;
use iced::subscription::events;
use iced::widget::{
    button, checkbox, column, container, horizontal_rule, horizontal_space, progress_bar, row,
    scrollable, text, vertical_rule, vertical_space, Text,
};
use iced::{
    executor, theme, time, window, Alignment, Application, Color, Command, Element, Length,
    Settings, Subscription, Theme,
};
use steamworks::{AppId, Client, ClientManager, PublishedFileId, UGC};
use tokio::sync::oneshot;
use humansize::{format_size, DECIMAL};

use crate::presets::{Mod, ModPreset, PresetParser};
use crate::workshop::Workshop;

pub mod presets;
pub mod widgets;
pub mod workshop;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct AMDU {
    // parser: Arc<Mutex<PresetParser>>,
    workshop: Option<Arc<Workshop>>,
    error: String,
    parser: PresetParser,
    mod_selection_list: Vec<ModRow>,
    mod_selection_index: Vec<usize>,
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

impl Application for AMDU {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        // let mut parser = Arc::new(Mutex::new(PresetParser::new()));

        let mut ws: Option<Arc<Workshop>> = None;
        let mut err: String = "".to_string();

        match Workshop::new(AppId(107410)) {
            Ok(result) => {ws = Option::from(Arc::new(result)) },
            Err(e) => { err = e }
        }

        (
            Self {
                workshop: ws,
                error: err,
                parser: PresetParser::new(),
                mod_selection_list: vec![],
                mod_selection_index: vec![],
                workshop_subbed_mods: vec![],
                toggle_all_state: true,
                unsub_in_progress: false,
                unsub_total_count: 0,
                unsub_progress: Arc::new(AtomicU32::new(0)),
            },
            Command::perform(init(), Message::Init),
        )
    }

    fn title(&self) -> String {
        String::from("AMDU")
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            events().map(Message::EventOccurred),
            // this is not the most optimal way to do it, but the polling ticker only runs during unsub progress... so should be fine
            match self.unsub_in_progress {
                false => Subscription::none(),
                true => time::every(Duration::from_millis(10)).map(Message::UnsubProgress),
            },
        ])
    }

    fn update(&mut self, message: Self::Message) -> Command<Message> {
        match message {
            Message::EventOccurred(event) => {
                // if window close event, we drop workshop as this will trigger cleanup for the spawned thread there
                if let Event::Window(window::Event::CloseRequested) = event {
                    self.workshop.as_ref().unwrap().thread_shutdown_signal.cancel();
                    window::close()
                } else {
                    Command::none()
                }
            }
            Message::Init(Ok(_)) => {
                // init called as app is started
                // Don't fetch anything if workshop could not be initialized
                match self.workshop.clone() {
                    Some(ws) => {
                        Command::perform(
                            load_subscribed_mods(ws),
                            Message::SubscribedModsFetched,
                        )
                    },
                    None => {
                        Command::none()
                    }
                }
            }
            Message::Init(Err(e)) => {
                // todo error?
                println!("init: Error {:?}", e);
                Command::none()
            }
            Message::SubscribedModsFetched(result) => {
                return match result {
                    Ok(mods) => {
                        self.workshop_subbed_mods = mods.to_vec();

                        Command::batch(vec![
                            Command::perform(
                                calculate_local_file_size(
                                    self.workshop_subbed_mods.clone(),
                                    self.workshop.clone().unwrap(),
                                ),
                                Message::LocalFileSizeFetched,
                            ),
                            Command::perform(
                                calculate_diff_mods(
                                    self.parser.get_modpresets(),
                                    self.workshop_subbed_mods.clone(),
                                ),
                                Message::UpdateSelectionView,
                            ),
                        ])
                    }
                    Err(e) => Command::none(),
                };
            }
            Message::UpdateSelectionView(diff_mods) => {
                // get diff, not calling as async as this is just straight vector diff and thus quick

                let mut mod_rows = vec![];
                for (i, item) in diff_mods.iter().enumerate() {
                    let row = ModRow::new(
                        item.id.clone(),
                        item.name.clone(),
                        item.url.clone(),
                        item.local_filesize.clone(),
                        true,
                    );
                    mod_rows.push(row);
                }
                self.mod_selection_list = mod_rows;

                Command::none()
            }
            Message::LocalFileSizeFetched(result) => {
                return match result {
                    Ok(mods) => {
                        self.workshop_subbed_mods = mods.to_vec();
                        // as we have updated data source now, update selection view by recalc
                        Command::perform(
                            calculate_diff_mods(
                                self.parser.get_modpresets(),
                                self.workshop_subbed_mods.clone(),
                            ),
                            Message::UpdateSelectionView,
                        )
                    }
                    Err(e) => {
                        println!("Failed fetching local install sizes");
                        Command::none()
                    }
                };
            }
            Message::OpenFileDialog => {
                println!("opening file dialog btn pressed");
                Command::perform(pick_files(), Message::FilesPicked)
            }
            Message::FilesPicked(Ok(content)) => Command::perform(
                PresetParser::load_files_async(content.to_vec()),
                Message::FilesParsed,
            ),
            Message::FilesPicked(Err(error)) => {
                println!("Error on files picked: {:?}", error);

                Command::none()
            }
            Message::FilesParsed(Ok(data)) => {
                // save parsed to own state
                self.parser.set_modpresets(data.to_vec()).unwrap();

                Command::perform(
                    calculate_diff_mods(
                        self.parser.get_modpresets(),
                        self.workshop_subbed_mods.clone(),
                    ),
                    Message::UpdateSelectionView,
                )
            }
            Message::FilesParsed(Err(error)) => {
                println!("Error on files parsed: {:?}", error);
                Command::none()
            }
            Message::List(index, msg) => {
                match msg {
                    RowMessage::ToggleSelection(toggle) => {
                        self.mod_selection_list[index.clone()].selected = toggle;
                        return Command::none();
                    }
                    RowMessage::ModPressed => {
                        // we do the same as toggle selection
                        self.mod_selection_list[index.clone()].selected =
                            !self.mod_selection_list[index.clone()].selected.clone();

                        return Command::none();
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

                Command::perform(
                    unsub_selected_mods(
                        self.mod_selection_list.clone(),
                        self.workshop.clone().unwrap(),
                        self.unsub_progress.clone(),
                    ),
                    Message::UnsubbedSelectedMods,
                )
            }
            Message::UnsubbedSelectedMods(result) => {
                self.unsub_in_progress = false;
                Command::perform(
                    load_subscribed_mods(self.workshop.clone().unwrap()),
                    Message::SubscribedModsFetched,
                )
            }
            Message::UnsubProgress(now) => {
                // self.unsub_progress = *progress;
                // just ticking gui update...
                Command::none()
            }
            Message::ToggleAll => {
                // toggle state
                self.toggle_all_state = !self.toggle_all_state;

                // update selection
                for val in self.mod_selection_list.iter_mut() {
                    val.selected = self.toggle_all_state;
                }

                Command::none()
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
                    .horizontal_alignment(Horizontal::Center)
                    .vertical_alignment(Vertical::Top),
                text("(Does not include subscribed scenarios)")
                    .width(Length::Fill)
                    .size(15)
                    .horizontal_alignment(Horizontal::Center)
                    .vertical_alignment(Vertical::Top),
                horizontal_rule(38),
                row![
                    text(format!("An Error Occured: {:?}", self.error))
                    .size(30)
                    .horizontal_alignment(Horizontal::Center)
                    .vertical_alignment(Vertical::Center)
                ]
                    .spacing(10)
                    .height(Length::FillPortion(400))
                    .align_items(Alignment::Center),
                row![
                    horizontal_space(Length::Fill),
                    text(format!("v{}", VERSION)).vertical_alignment(Vertical::Bottom).horizontal_alignment(Horizontal::Right)
                ].align_items(Alignment::End)
            ]
                .spacing(5)
                .padding(20)
                .align_items(Alignment::Start);

            return container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x()
                .center_y()
                .into();
        }

        // MAIN CONTENT
        let load_presets = column![
            text("Load presets you wish to keep")
                .horizontal_alignment(Horizontal::Center)
                .vertical_alignment(Vertical::Top),
            vertical_space(15),
            button("Load Presets")
                .padding(10)
                .on_press(Message::OpenFileDialog),
        ]
        .padding([5, 5])
        .align_items(Alignment::Center)
        .height(150);

        // get loaded presets name only
        let p_loaded: Vec<String> = self
            .parser
            .get_modpresets()
            .iter()
            .map(|p| p.name.clone())
            .collect();
        let scrollable_presets = scrollable(
            p_loaded.iter().fold(
                column![]
                    .spacing(6)
                    // .padding(1)
                    .align_items(Alignment::Center)
                    .width(Length::Fill),
                |col, i| col.push(text(i).horizontal_alignment(Horizontal::Center)),
            ),
        )
        .width(Length::Fill)
        .height(100);
        // .direction(Direction::Vertical(Properties::new().alignment(scrollable::Alignment::Start)))
        // .height(70);

        let presets_loaded = column![
            text("Loaded Presets")
                .horizontal_alignment(Horizontal::Center)
                .vertical_alignment(Vertical::Top),
            vertical_space(2),
            horizontal_rule(2),
            vertical_space(6),
            scrollable_presets,
        ]
        .padding([5, 5])
        .align_items(Alignment::Center)
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
                    .horizontal_alignment(Horizontal::Right),
            ]
            .align_items(Alignment::Start)
            .spacing(30),
            row![
                text("Mods to be removed:").width(Length::FillPortion(5)),
                text(format!("{:}", selected_mods_count)).horizontal_alignment(Horizontal::Right),
            ]
            .align_items(Alignment::Start)
            .spacing(30),
            row![
                text("Space that will free up:").width(Length::FillPortion(5)),
                text(format_size(subscribed_mods_local_size_sum, DECIMAL))
                    .horizontal_alignment(Horizontal::Right),
            ]
            .align_items(Alignment::Start)
            .spacing(30),
        ]
        .padding([5, 5])
        .spacing(20)
        .align_items(Alignment::Start)
        .height(150)
        .width(300);

        let mut unsub_button = button(
            row![text("Unsub Selected Mods")
                .width(Length::Fill)
                .vertical_alignment(Vertical::Center)
                .horizontal_alignment(Horizontal::Center)]
            .align_items(Alignment::Center),
        )
        .padding([5, 5])
        .width(150)
        .height(150);

        if self.mod_selection_list.len() > 0 {
            unsub_button = unsub_button.on_press(Message::UnsubSelected);
        }

        let selection_list = self.mod_selection_list.iter().enumerate().fold(
            column![].spacing(6),
            |col, (i, item)| {
                col.push(
                    self.mod_selection_list[i]
                        .view()
                        .map(move |msg| Message::List(i, msg)),
                )
            },
        );

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
                .horizontal_alignment(Horizontal::Center)
                .vertical_alignment(Vertical::Top),
                progress_bar(
                    0.0..=self.unsub_total_count as f32,
                    self.unsub_progress.load(Ordering::Relaxed) as f32
                )
            ]
            .spacing(5)
            .align_items(Alignment::Center)
            .padding(10)
            .into(),
        };

        let bottom_bar = row![
            button("Toggle All")
                .padding(10)
                .on_press(Message::ToggleAll),
            horizontal_space(Length::Fill),
            text(format!("v{}", VERSION)).vertical_alignment(Vertical::Bottom)
        ]
        .padding(5);

        let content = column![
            text("Arma3 Mod Differential Unsubscriber")
                .width(Length::Fill)
                .size(40)
                .horizontal_alignment(Horizontal::Center)
                .vertical_alignment(Vertical::Top),
            text("(Does not include subscribed scenarios)")
                .width(Length::Fill)
                .size(15)
                .horizontal_alignment(Horizontal::Center)
                .vertical_alignment(Vertical::Top),
            horizontal_rule(38),
            row![
                load_presets,
                vertical_rule(2),
                presets_loaded,
                vertical_rule(2),
                mods_stats,
                vertical_rule(2),
                horizontal_space(5),
                unsub_button,
            ]
            .spacing(8)
            .align_items(Alignment::Center)
            .height(160),
            horizontal_rule(38),
            row![scrollable]
                .spacing(10)
                .height(Length::FillPortion(400))
                .align_items(Alignment::Center),
            bottom_bar.align_items(Alignment::Start),
        ]
        .spacing(5)
        .padding(20)
        .align_items(Alignment::Start);

        return container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into();
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

#[derive(Debug, Clone)]
enum Error {
    DialogClosed,
    IO(io::ErrorKind),
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
    return Ok(Arc::new(vector_paths));
    // let arc: Arc<[Path]> = vector_paths.into()
}

async fn init() -> Result<(), String> {
    // run when created, for init code
    return Ok(());
}

async fn load_subscribed_mods(
    workshop: Arc<Workshop>,
) -> Result<Arc<Vec<Mod>>, oneshot::error::RecvError> {
    let mods = workshop.get_subscribed_mods_info().await.unwrap();
    let formatted_mods = mods
        .iter()
        .filter(|item| !item.tags.contains(&"Scenario".to_owned()))
        .map(|result| Mod {
            id: result.published_file_id.0.clone(),
            url: result.url.clone(),
            tags: result.tags.clone(),
            name: result.title.clone(),
            local_filesize: 0,
        })
        .collect();
    return Ok(Arc::new(formatted_mods));
}

async fn calculate_diff_mods(keep_sets: Vec<ModPreset>, all_mods: Vec<Mod>) -> Arc<Vec<Mod>>
where
    Mod: std::cmp::Ord,
{
    // if we have empty all_mods, we should return empty diff
    if keep_sets.len() <= 0 {
        return Arc::new(all_mods);
    }

    let combined_keep_mods: Vec<_> = keep_sets
        .iter()
        .map(|item| item.mods.clone())
        .flatten()
        .collect();
    let keep_mods_set = BTreeSet::from_iter(combined_keep_mods);

    let mut diff_mods: Vec<Mod> = all_mods.clone();
    diff_mods.sort();
    diff_mods.retain(|e| !keep_mods_set.contains(e));

    // sleep we need due to bug on windows causing some batch commands not run if return too fast: https://github.com/iced-rs/iced/issues/436
    tokio::time::sleep(Duration::from_millis(2)).await;

    return Arc::new(diff_mods);
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

    return Ok(Arc::new(mods));
}

async fn unsub_selected_mods(
    mods: Vec<ModRow>,
    workshop: Arc<Workshop>,
    mut progress: Arc<AtomicU32>,
) -> Result<(), String> {
    for (i, val) in mods.iter().filter(|item| item.selected).enumerate() {
        // for every loop we add one to progress to show what mod we are currently unsubbing
        progress.fetch_add(1, Ordering::Relaxed);

        match workshop.unsub_from_mod(PublishedFileId(val.id)).await {
            // TODO make subscription for return. Idea is we show progress bar while going through it, and on each ok/err we publish progress?
            Ok(_) => {}
            Err(e) => {}
        }
    }
    Ok(())
}

pub fn main() -> iced::Result {
    AMDU::run(Settings {
        exit_on_close_request: false,
        ..Settings::default()
    })
}
