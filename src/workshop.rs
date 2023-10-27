#![deny(clippy::all)]

use std::sync::{mpsc};
use steamworks::{Client, PublishedFileId, QueryResult};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

pub struct Workshop {
    client: steamworks::Client,
    pub thread_shutdown_signal: CancellationToken,
}

impl Workshop {
    pub fn new(app_id: steamworks::AppId) -> Result<Self, String> {
        // try to initialize client
        let client_result = Client::init_app(app_id);
        if client_result.is_err() {
            return Err("Failed to initialize steam client. Make sure steam is running in the background and restart AMDU".to_string());
        }

        // if client is ok, we save it
        let (client, single) = client_result.unwrap();

        // make thread for callback running
        let token = CancellationToken::new();
        let cloned_token = token.clone();

        // create a thread for callbacks
        tokio::spawn(async move {
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
            client,
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
                println!("Error on making subscribed items query, ERR: {:?}", e)
            }
        }
        Ok(receiver.recv().unwrap())
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
        receiver.recv().unwrap() // If we want to handle both steamerror and oneshot error, use crate AnyHow. Has result types that can easily be converted to.
    }
}

impl Drop for Workshop {
    fn drop(&mut self) {
        self.thread_shutdown_signal.cancel();
    }
}
