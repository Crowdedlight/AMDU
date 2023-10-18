use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicPtr;

use iced::{Alignment, Application, Color, Command, Element, executor, Length, Settings, Theme, theme};
use iced::alignment::{Horizontal, Vertical};
use iced::font::load;
use iced::futures::future::ok;
use iced::futures::StreamExt;
use iced::theme::Button;
use iced::widget::{button, column, checkbox, container, row, scrollable, text, vertical_rule, horizontal_rule, horizontal_space, vertical_space, Text};
use iced::widget::scrollable::{Direction, Properties};
use steamworks::{AppId, Client, ClientManager, PublishedFileId, UGC};
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;
use crate::widgets::modrow::{Message as RowMessage, ModRow};
// use crate::mod_row::ModRow::{Message as RowMessage, ModRow};

use crate::presets::{Mod, ModPreset, PresetParser};
use crate::workshop::Workshop;

pub mod workshop;
pub mod presets;
pub mod widgets;

struct AMDU {
    // parser: Arc<Mutex<PresetParser>>,
    parser: PresetParser,
    mod_selection_list: Vec<ModRow>,
    mod_selection_index: Vec<usize>,
}

#[derive(Debug, Clone)]
enum Message {
    // FilesOpened(Result<PresetParser, String>),
    OpenFileDialog,
    FilesPicked(Result<Arc<Vec<PathBuf>>, Error>),
    FilesParsed(Result<Arc<Vec<ModPreset>>, String>),
    List(usize, RowMessage),
}

impl Application for AMDU {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {

        // let mut parser = Arc::new(Mutex::new(PresetParser::new()));

        (Self {parser: PresetParser::new(), mod_selection_list: vec![], mod_selection_index: vec![] },
         Command::none())
    }

    fn title(&self) -> String {
        String::from("AMDU")
    }

    fn update(&mut self, message: Self::Message) -> Command<Message> {
        match message {
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
                Command::perform(PresetParser::load_files_async(content.to_vec()), Message::FilesParsed)
            }
            Message::FilesPicked(Err(error)) => {
                println!("Error on files picked: {:?}", error);

                Command::none()
            }
            Message::FilesParsed(Ok(data)) => {
                // save parsed to own state
                self.parser.set_modpresets(data.to_vec()).unwrap();

                for val in self.parser.get_modpresets() {
                    println!("{:?}", val);
                }

                // TODO debug, making list of Modrows based on mod preset and saving
                let mut mod_rows = vec![];
                let mut mod_index = vec![];
                for (i, item) in self.parser.get_modpresets()[0].mods.iter().enumerate() {
                    let row = ModRow::new(item.name.clone(), item.url.clone(), true);
                    mod_rows.push(row);
                    mod_index.push(i)
                };
                self.mod_selection_index = Vec::from_iter(0..mod_rows.len());
                self.mod_selection_list = mod_rows;

                println!("{:?}; {:?}", self.mod_selection_index.len(), self.mod_selection_list.len());

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
                // TODO match on message types
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {

        let load_presets = column![
                text("Load presets you wish to keep").horizontal_alignment(Horizontal::Center).vertical_alignment(Vertical::Top),
                vertical_space(15),
                button("Load Presets").padding(10).on_press(Message::OpenFileDialog),
            ].padding([5, 5]).align_items(Alignment::Center).height(150);

        // get loaded presets name only
        let p_loaded: Vec<String> = self.parser.get_modpresets().iter().map(|p| p.name.clone()).collect();
        let scrollable_presets = scrollable(
            p_loaded.iter()
                .fold(column![]
                          .spacing(6)
                          // .padding(1)
                          .align_items(Alignment::Center)
                          .width(Length::Fill),
                  |col, i| {
                        col.push(
                            text(i).horizontal_alignment(Horizontal::Center),
                        )
                })
        )
            .width(Length::Fill)
            .height(100);
            // .direction(Direction::Vertical(Properties::new().alignment(scrollable::Alignment::Start)))
            // .height(70);

        let presets_loaded = column![
                text("Loaded Presets").horizontal_alignment(Horizontal::Center).vertical_alignment(Vertical::Top),
                vertical_space(2),
                horizontal_rule(2),
                vertical_space(6),
                scrollable_presets,
            ].padding([5, 5]).align_items(Alignment::Center).height(150).max_width(220);

        // todo replace with real variables
        let mods_stats = column![
                text(format!("Mods subscribed to:      {:>8}", 120)),
                text(format!("Mods to be removed:     {:>8}", 20)),
                text(format!("Space that will free up: {:>8.1} MB", 4267.5)),
            ].padding([5, 5]).spacing(20).align_items(Alignment::Start).height(150).width(300);

        let mut unsub_button = button(
            row![
                text("Unsub Selected Mods").width(Length::Fill).vertical_alignment(Vertical::Center).horizontal_alignment(Horizontal::Center)
            ].align_items(Alignment::Center)
        ).padding([5, 5]).width(150).height(150);
        // TODO enable if we allow unsubbing
        // if self.selectedMods.length() > 0 {
        //     unsub_button.on_press();
        // }

        // TODO
        let selection_list = self.mod_selection_index.iter().fold(column![].spacing(6), |col, i| {
            col.push(
                self.mod_selection_list[*i]
                    .view()
                    .map(move |msg| Message::List(*i, msg)))
        });

        let scrollable = scrollable(selection_list)
            .width(Length::Fill)
            .height(Length::Fill);

        let version_bar = row![horizontal_space(Length::Fill), "v0.1.0"].padding(5);

        let content = column![
            text("Arma3 Mod Differential Unsubscriber")
                .width(Length::Fill)
                .size(40)
                .horizontal_alignment(Horizontal::Center).vertical_alignment(Vertical::Top),
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
            row![
                scrollable,
            ]
            .spacing(10)
            .height(Length::FillPortion(400))
            .align_items(Alignment::Center),
            version_bar.align_items(Alignment::End),
        ]
            .spacing(20)
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
    IO(io::ErrorKind)
}

async fn pick_files() -> Result<Arc<Vec<PathBuf>>, Error> {
    let paths = rfd::AsyncFileDialog::new()
        .add_filter("html", &["html"])
        .set_directory("/")
        .set_title("Pick Preset Files")
        .pick_files()
        .await.ok_or(Error::DialogClosed)?;

    let vector_paths = paths.iter()
        .map(|handle| handle.path().to_path_buf())
        .collect();
    return Ok(Arc::new(vector_paths))
    // let arc: Arc<[Path]> = vector_paths.into()
}


pub fn main() -> iced::Result {

    AMDU::run(Settings::default())












    // parse presets
    // let input = vec!["C:\\Users\\crow\\Documents\\Github\\amdu\\test\\test.html".to_string()];
    // let presets = PresetParser::new(input).expect("Failed parsing files into presets");
    // let ids = presets.get_all_mod_ids_unique().unwrap();
    // println!("{:?}", ids);


    // create workshop
    // let ws = Workshop::new(AppId(107410)).await.unwrap();
    //
    // // various test callls
    // let utils = ws.client().utils();
    // println!("Utils:");
    // println!("AppId: {:?}", utils.app_id());
    // println!("UI Language: {}", utils.ui_language());
    //
    // let apps = ws.client().apps();
    // println!("Apps");
    // println!("IsInstalled(107410): {}", apps.is_app_installed(AppId(107410)));
    // println!("InstallDir(107410): {}", apps.app_install_dir(AppId(107410)));
    // println!("BuildId: {}", apps.app_build_id());
    // println!("AppOwner: {:?}", apps.app_owner());
    // println!("Langs: {:?}", apps.available_game_languages());
    // println!("Lang: {}", apps.current_game_language());
    // println!("Beta: {:?}", apps.current_beta_name());


    // let all_subbed_mods = ws.get_subscribed_mods_info().await.expect("PANIC: error returning allmods result from callback thread to main thread");
    // for item in all_subbed_mods {
    //     let filesize_on_disk = ws.get_item_install_info(item.published_file_id).unwrap();
    //     println!("ID: {:?}, Titel: {:?}, file_size: {:.1}MB, file_size_disk: {:.1}MB", item.published_file_id.0, item.title, (item.file_size as f64 / 1e6), (filesize_on_disk.size_on_disk as f64 / 1e6))
    // };

    // unsubscribe from a mod // Deformer as test mod: 2822758266 - https://steamcommunity.com/workshop/filedetails/?id=2822758266
    // let unsub_result = ws.unsub_from_mod(PublishedFileId(28)).await;
    // match unsub_result {
    //     Ok(_) => {println!("Unsubbed successfully from: {:?}", 28 as u32)},
    //     Err(e) => println!("Error unsubbing from mod id: {:?}, error: {:?}", 28 as u32, e)
    // };

    // TODO add function that returns the mod IDs that you are subscribed to, but doesn't exist on the workshop anymore?
    //  would be a combo of getting "subscribed items", and either doing a query per to see what returns info, alternative we could compare "allmods" id list with "get subscribed items" list. Any ids not present in both, is unavailable mod.


    // drop struct to finish dangling threads
    // ws.stop_cb_thread().await;
}
