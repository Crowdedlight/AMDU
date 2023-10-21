#![deny(clippy::all)]

use std::collections::BTreeSet;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicPtr;
use std::sync::{Arc, Mutex};

use crate::widgets::modrow::{Message as RowMessage, ModRow};
use iced::alignment::{Horizontal, Vertical};
use iced::event::{self, Event};
use iced::font::load;
use iced::futures::future::ok;
use iced::futures::StreamExt;
use iced::subscription::events;
use iced::theme::Button;
use iced::theme::Svg::Default;
use iced::widget::scrollable::{Direction, Properties};
use iced::widget::{
    button, checkbox, column, container, horizontal_rule, horizontal_space, row, scrollable, text,
    vertical_rule, vertical_space, Text,
};
use iced::{
    executor, theme, window, Alignment, Application, Color, Command, Element, Length, Settings,
    Subscription, Theme,
};
use steamworks::{AppId, Client, ClientManager, PublishedFileId, UGC};
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;

use humansize::{format_size, DECIMAL};

use crate::presets::{Mod, ModPreset, PresetParser};
use crate::workshop::Workshop;

pub mod presets;
pub mod widgets;
pub mod workshop;

struct AMDU {
    // parser: Arc<Mutex<PresetParser>>,
    workshop: Arc<Workshop>,
    parser: PresetParser,
    mod_selection_list: Vec<ModRow>,
    mod_selection_index: Vec<usize>,
    workshop_subbed_mods: Vec<Mod>,
    toggle_all_state: bool,
}

#[derive(Debug, Clone)]
enum Message {
    EventOccurred(Event),
    OpenFileDialog,
    FilesPicked(Result<Arc<Vec<PathBuf>>, Error>),
    FilesParsed(Result<Arc<Vec<ModPreset>>, String>),
    List(usize, RowMessage),
    UnsubSelected,
    SubscribedModsFetched(Result<Arc<Vec<Mod>>, oneshot::error::RecvError>),
    LocalFileSizeFetched(Result<Arc<Vec<Mod>>, String>),
    Init(Result<(), String>),
    ToggleAll,
    UnsubbedSelectedMods(Result<(), String>),
}

impl Application for AMDU {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        // let mut parser = Arc::new(Mutex::new(PresetParser::new()));

        (
            Self {
                workshop: Arc::new(Workshop::new(AppId(107410)).unwrap()),
                parser: PresetParser::new(),
                mod_selection_list: vec![],
                mod_selection_index: vec![],
                workshop_subbed_mods: vec![],
                toggle_all_state: true,
            },
            Command::perform(init(), Message::Init),
        )
    }

    fn title(&self) -> String {
        String::from("AMDU")
    }

    fn subscription(&self) -> Subscription<Message> {
        events().map(Message::EventOccurred)
    }

    fn update(&mut self, message: Self::Message) -> Command<Message> {
        match message {
            Message::EventOccurred(event) => {
                // if window close event, we drop workshop as this will trigger cleanup for the spawned thread there
                if let Event::Window(window::Event::CloseRequested) = event {
                    self.workshop.thread_shutdown_signal.cancel();
                    window::close()
                } else {
                    Command::none()
                }
            }
            Message::Init(Ok(_)) => {
                // init called as app is started
                Command::perform(
                    load_subscribed_mods(self.workshop.clone()),
                    Message::SubscribedModsFetched,
                )
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
                        Command::perform(
                            calculate_local_file_size(
                                self.workshop_subbed_mods.clone(),
                                self.workshop.clone(),
                            ),
                            Message::LocalFileSizeFetched,
                        )
                    }
                    Err(e) => Command::none(),
                };
            }
            Message::LocalFileSizeFetched(result) => {
                return match result {
                    Ok(mods) => {
                        self.workshop_subbed_mods = mods.to_vec();
                        Command::none()
                    }
                    Err(e) => {
                        println!("Failed fetching local install sizes");
                        Command::none()
                    }
                }
            }
            Message::OpenFileDialog => {
                println!("opening file dialog btn pressed");
                Command::perform(pick_files(), Message::FilesPicked)
            }
            Message::FilesPicked(Ok(content)) => {
                for val in content.iter() {
                    println!("{:?}", val);
                }
                // let self_clone = Arc::clone(&self.parser);
                // Command::perform(self_clone.lock().unwrap().load_files(content.to_vec()), Message::FilesParsed);
                // TODO figure out how to do it as command and async... Caused heaps of issues to call a self member function async...
                //  I cannot mutate Parser in async command, however I can get it to respond with the results of the command, so in this case it would respond with a Vec<ModPreset>, that I can then save in the state...
                // let result = self.parser.load_files(content.to_vec());
                Command::perform(
                    PresetParser::load_files_async(content.to_vec()),
                    Message::FilesParsed,
                )
            }
            Message::FilesPicked(Err(error)) => {
                println!("Error on files picked: {:?}", error);

                Command::none()
            }
            Message::FilesParsed(Ok(data)) => {
                // save parsed to own state
                self.parser.set_modpresets(data.to_vec()).unwrap();

                // TODO debug, making list of Modrows based on mod preset and saving
                // TODO command::perform for function to take both vectors and return diff
                // self.parser.get_modpresets()

                // TODO Should probably call a message in some way as "UpdateDiff()" as we want to redo the diff when sub is refreshed, or preset is reloaded
                // get diff, not calling as async as this is just straight vector diff and thus quick
                let diff_mods = calculate_diff_mods(
                    self.parser.get_modpresets(),
                    self.workshop_subbed_mods.clone(),
                );

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

                // TODO set own state with contents from preset to show in the list
                //  should probably auto load all workshop subscribed mods when the app loads
                //  and then automatically populate the un-needed here for the list
                Command::none()
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
            Message::UnsubSelected => Command::perform(
                unsub_selected_mods(self.mod_selection_list.clone(), self.workshop.clone()),
                Message::UnsubbedSelectedMods,
            ),
            Message::UnsubbedSelectedMods(result) => {
                // TODO right now we only perform the mod sub again, we don't actully sync the diff. We should move diff out of parsed-files, and call it whenever
                //  preset, or subscribed-mods are updated. Adding a check if preset files are parsed, otherwise just exit
                Command::perform(
                    load_subscribed_mods(self.workshop.clone()),
                    Message::SubscribedModsFetched,
                )
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
        .max_width(220);

        // todo replace with real variables
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

        let scrollable = scrollable(selection_list)
            .width(Length::Fill)
            .height(Length::Fill);

        let bottom_bar = row![
            button("Toggle All")
                .padding(10)
                .on_press(Message::ToggleAll),
            horizontal_space(Length::Fill),
            "v0.1.0"
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
            row![scrollable,]
                .spacing(10)
                .height(Length::FillPortion(400))
                .align_items(Alignment::Center),
            bottom_bar.align_items(Alignment::Start),
        ]
        .spacing(5)
        .padding(20)
        .align_items(Alignment::Start);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
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

fn calculate_diff_mods(keep_sets: Vec<ModPreset>, mut all_mods: Vec<Mod>) -> Vec<Mod>
where
    Mod: std::cmp::Ord,
{
    let combined_keep_mods: Vec<_> = keep_sets
        .iter()
        .map(|item| item.mods.clone())
        .flatten()
        .collect();
    let to_remove = BTreeSet::from_iter(combined_keep_mods);

    all_mods.retain(|e| !to_remove.contains(e));
    return all_mods;
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

    return Ok(Arc::new(mods));
}

async fn unsub_selected_mods(mods: Vec<ModRow>, workshop: Arc<Workshop>) -> Result<(), String> {
    for val in mods.iter().filter(|item| item.selected) {
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
