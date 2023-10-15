use steamworks::{AppId, Client, ClientManager, PublishedFileId, UGC};
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;
use crate::presets::PresetParser;
use crate::workshop::Workshop;

pub mod workshop;
pub mod presets;

#[tokio::main]
async fn main() {

    // parse presets
    let input = vec!["C:\\Users\\crow\\Documents\\Github\\amdu\\test\\test.html".to_string()];
    let presets = PresetParser::new(input).expect("Failed parsing files into presets");
    let ids = presets.get_all_mod_ids_unique().unwrap();
    println!("{:?}", ids);


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
