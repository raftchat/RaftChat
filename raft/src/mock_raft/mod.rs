use crate::database::DB;
use crate::raftchat_tonic::{Entry, UserRequestArgs};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

pub struct Raft {}

struct RaftConfig {
    id: u64,
    peers: Vec<String>,
}

struct RaftNode {
    id: u64,
    config: RaftConfig,
    commit_tx: mpsc::Sender<Entry>,
    propose_rx: mpsc::Receiver<UserRequestArgs>,
    test_flag: bool,
    client_timestamp_map: std::collections::HashMap<String, u64>,
}

impl DB for Raft {
    // return a new commit channel
    fn make_channel(
        &self,
        id: u64,
        peers: Vec<String>,
    ) -> (mpsc::Receiver<Entry>, mpsc::Sender<UserRequestArgs>) {
        let (commit_tx, commit_rx) = mpsc::channel(15);
        let (propose_tx, propose_rx) = mpsc::channel(15);

        let mut raft_node = RaftNode {
            id: id,
            config: RaftConfig {
                id: id,
                peers: peers,
            },
            commit_tx: commit_tx,
            propose_rx: propose_rx,
            test_flag: true,
            client_timestamp_map: std::collections::HashMap::new(),
        };

        tokio::spawn(async move {
            raft_node.start().await;
        });

        // tokio::task::spawn_blocking(move || {
        //     tokio::runtime::Handle::current().block_on(async {
        //         raft_node.start().await;
        //     });
        // });

        return (commit_rx, propose_tx);
    }
}

impl RaftNode {
    pub async fn start(&mut self) {
        //println!("Starting Raft server with id: {}", self.id);

        let mut idx = 0;
        let mut drop = 3;

        // [NOTE] This is a dummy implementation
        // echo back the data
        while let Some(data) = self.propose_rx.recv().await {
            //println!("[RAFT] Received data");

            sleep(Duration::from_secs(1)).await;

            // now msg is committed and wirtten on disk.

            let time = self.client_timestamp_map.get(&data.client_id).unwrap_or(&1);

            if self.test_flag && idx == drop {
                drop += 100;
                continue;
            }

            // filter duplciate requests and out of order requests
            if *time != data.message_id {
                continue;
            }

            self.client_timestamp_map
                .insert(data.client_id.clone(), *time + 1);

            let value = Entry {
                term: 0,
                client_id: data.client_id,
                message_id: data.message_id,
                data: data.data,
            };

            self.commit_tx.send(value).await.unwrap();
            idx += 1;
        }

        // [TODO] Implement WAL and Transport
        // setup the WAL and Transport
    }
}
