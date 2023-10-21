use std::fmt::format;
use std::sync::mpsc::RecvError;
use std::sync::{mpsc, Arc};
use steamworks::{AppId, Client, ClientManager, PublishedFileId, QueryResult, UGC};
use tokio::select;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::TryRecvError;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct Workshop {
    app_id: steamworks::AppId,
    client: steamworks::Client,
    thread: JoinHandle<()>,
    pub thread_shutdown_signal: CancellationToken,
}

impl Workshop {
    pub fn new(app_id: steamworks::AppId) -> Result<Self, String> {
        // try to initialize client
        let client_result = Client::init();
        if client_result.is_err() {
            return Err("Failed to initialize steam client".to_string());
        }

        // if client is ok, we save it
        let (client, single) = client_result.unwrap();

        if !client.apps().is_app_installed(app_id) {
            return Err(format!(
                "Selected app is not installed... AppId: {}",
                app_id.0
            ));
        }

        // make thread for callback running
        let token = CancellationToken::new();
        let cloned_token = token.clone();

        // create a thread for callbacks
        let thread = tokio::spawn(async move {
            loop {
                // run callbacks
                single.run_callbacks();
                std::thread::sleep(std::time::Duration::from_millis(100));

                if cloned_token.is_cancelled() {
                    println!("Received Exit-signal breaking callback thread");
                    break;
                }
            }
        });

        Ok(Workshop {
            app_id,
            client,
            thread,
            thread_shutdown_signal: token,
        })
    }

    pub fn client(&self) -> &steamworks::Client {
        &self.client
    }

    pub fn get_subscribed_items(&self) -> Vec<steamworks::PublishedFileId> {
        self.client.ugc().subscribed_items()
    }

    pub fn get_item_install_info(
        &self,
        item_id: PublishedFileId,
    ) -> Option<steamworks::InstallInfo> {
        self.client.ugc().item_install_info(item_id)
    }

    pub async fn get_subscribed_mods_info(
        &self,
    ) -> Result<Vec<QueryResult>, oneshot::error::RecvError> {
        // get list of subscribed mods
        let list = self.get_subscribed_items();

        // make signals, not using tokio as that apperently didn't work with this closure...
        let (sender, receiver) = mpsc::channel();

        match self.client.ugc().query_items(list) {
            Ok(item_list_query) => {
                item_list_query.fetch(move |query_result| {
                    match query_result {
                        Ok(res) => {
                            // flatten to unpack all options/results and return only Some()
                            let result: Vec<QueryResult> = res.iter().flatten().collect();
                            // let main thread know we are done
                            sender
                                .send(result.clone())
                                .expect("PANIC: Main thread is gone");
                        }
                        Err(e) => {
                            println!("Error on query fetch: {:?}", e)
                        }
                    }
                })
            }
            Err(e) => {
                println!("Error on making subscribed items query")
            }
        }
        return Ok(receiver.recv().unwrap());
    }

    pub async fn unsub_from_mod(
        &self,
        item_id: PublishedFileId,
    ) -> Result<(), steamworks::SteamError> {
        // create signals
        let (sender, receiver) = mpsc::channel();

        // call unsub
        self.client
            .ugc()
            .unsubscribe_item(item_id, move |unsub_result| {
                sender
                    .send(unsub_result)
                    .expect("PANIC: Main thread is gone");
            });
        return receiver.recv().unwrap(); // If we want to handle both steamerror and oneshot error, use crate AnyHow. Has result types that can easily be converted to.
    }
}

impl Drop for Workshop {
    fn drop(&mut self) {
        self.thread_shutdown_signal.cancel();
    }
}
