use steamworks::{AppId, Client, ClientManager, PublishedFileId, UGC};
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;
use crate::workshop::Workshop;

pub mod workshop;
pub mod presets;

#[tokio::main]
async fn main() {

    // create workshop
    let ws = Workshop::new(AppId(107410)).await.unwrap();

    // various test callls
    let utils = ws.client().utils();
    println!("Utils:");
    println!("AppId: {:?}", utils.app_id());
    println!("UI Language: {}", utils.ui_language());

    let apps = ws.client().apps();
    println!("Apps");
    println!("IsInstalled(107410): {}", apps.is_app_installed(AppId(107410)));
    println!("InstallDir(107410): {}", apps.app_install_dir(AppId(107410)));
    println!("BuildId: {}", apps.app_build_id());
    println!("AppOwner: {:?}", apps.app_owner());
    println!("Langs: {:?}", apps.available_game_languages());
    println!("Lang: {}", apps.current_game_language());
    println!("Beta: {:?}", apps.current_beta_name());


    let all_subbed_mods = ws.get_subscribed_mods_info().await.unwrap();
    for item in all_subbed_mods {
        let filesize_on_disk = ws.get_item_install_info(item.published_file_id).unwrap();
        println!("ID: {:?}, Titel: {:?}, file_size: {:.1}MB, file_size_disk: {:.1}MB", item.published_file_id.0, item.title, (item.file_size as f64 / 1e6), (filesize_on_disk.size_on_disk as f64 / 1e6))
    };

    // unsubscribe from a mod // Deformer as test mod: 2822758266 - https://steamcommunity.com/workshop/filedetails/?id=2822758266
    // let unsub_result = ws.unsub_from_mod(PublishedFileId(2822758266)).await;
    // match unsub_result {
    //     Ok(_) => {println!("Unsubbed successfully from: {:?}", 2822758266 as u32)},
    //     Err(e) => println!("Error unsubbing from mod id: {:?}, error: {:?}", 2822758266 as u32, e)
    // };


    // drop struct to finish dangling threads
    ws.stop_cb_thread().await;
}
