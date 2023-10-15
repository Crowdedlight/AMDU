use std::fmt::format;
use std::sync::mpsc::RecvError;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;
use steamworks::{AppId, Client, ClientManager, PublishedFileId, QueryResult, UGC};
use tokio::task::JoinHandle;

pub struct Workshop {
    app_id: steamworks::AppId,
    client: steamworks::Client,
    thread: JoinHandle<()>,
    thread_shutdown_signal: oneshot::Sender<u8>,
}

impl Workshop {
    pub async fn new(
        app_id: steamworks::AppId,
    ) -> Result<Self, String> {

        // try to initialize client
        let client_result = Client::init();
        if client_result.is_err() {
            return Err("Failed to initialize steam client".to_string());
        }

        // if client is ok, we save it
        let (client, single) = client_result.unwrap();

        if !client.apps().is_app_installed(app_id) {
            return Err(format!("Selected app is not installed... AppId: {}", app_id.0));
        }

        // make thread for callback running
        let (thread_shutdown_signal, mut rx) = oneshot::channel::<u8>();
        // create a thread for callbacks
        // if you have an active loop (like in a game), you can skip this and just run the callbacks on update
        let thread = tokio::spawn(async move {
            loop {
                // run callbacks
                single.run_callbacks();
                std::thread::sleep(std::time::Duration::from_millis(100));

                // check if the channel is closed or if there is a message
                // end the thread if either is true
                match rx.try_recv() {
                    Ok(msg) => {println!("Received Exit-signal: {:?}, breaking callback thread", msg); break},
                    Err(TryRecvError::Empty) => {},
                    Err(_) => {println!("callback thread exiting: the sender dropped"); break},
                }
            }
        });

        Ok(Workshop {app_id, client, thread, thread_shutdown_signal})
    }

    pub async fn stop_cb_thread(self) {
        // we are dropping, so exit our spawned thead
        self.thread_shutdown_signal.send(1).expect("Error on dropping workshop callback thread");
        self.thread.await.expect("Error on joining workshop callback thread");
    }

    pub fn client(&self) -> &steamworks::Client {
        &self.client
    }

    pub fn get_subscribed_items(&self) -> Vec<steamworks::PublishedFileId> {
        self.client.ugc().subscribed_items()
    }

    pub fn get_item_install_info(&self, item_id: PublishedFileId) -> Option<steamworks::InstallInfo> {
        self.client.ugc().item_install_info(item_id)
    }

    pub async fn get_subscribed_mods_info(&self) -> Result<Vec<QueryResult>, oneshot::error::RecvError> {
        // get list of subscribed mods
        let list = self.get_subscribed_items();

        // make signals
        let (tx, mut rx) = oneshot::channel::<Vec<QueryResult>>();

        let query = self.client.ugc().query_items(list);
        match query {
            Ok(item_list_query) =>
                {
                    item_list_query.fetch(move |query_result| {
                        match query_result {
                            Ok(res) => {
                                // flatten to unpack all options/results and return only Some()
                                let result = res.iter().flatten().collect();
                                // let main thread know we are done
                                tx.send(result).expect("PANIC: Main thread is gone");
                            }
                            Err(e) => { println!("Error on query fetch: {:?}", e) }
                        }
                    })
                }
            Err(e) => { println!("Error on making subscribed items query") }
        }
        return rx.await
    }

    pub async fn unsub_from_mod(&self, item_id: PublishedFileId) -> Result<(), steamworks::SteamError> {
        // create signals
        let (tx, mut rx) = oneshot::channel::<Result<(), steamworks::SteamError>>();

        // call unsub
        self.client.ugc().unsubscribe_item(item_id, move |unsub_result| {
            tx.send(unsub_result).expect("PANIC: Main Thread is gone");
        });
        return rx.await.unwrap(); // If we want to handle both steamerror and oneshot error, use crate AnyHow. Has result types that can easily be converted to.
    }
}
