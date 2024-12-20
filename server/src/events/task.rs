use crate::data_model::msg::{ClientMsg, LogData, Msg, ServerMsg};
use chrono::Local;
use futures_util::stream::SplitSink;
use futures_util::SinkExt;
use log::{debug, info, Log};
use raft::raftchat_tonic::{Command, Entry, UserRequestArgs};
use std::collections::HashMap;
use std::sync::Arc;
use std::u64;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::time::{self, Duration};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

type Stream = SplitSink<WebSocketStream<TcpStream>, Message>;

// Writer task
// - Receives messages from the client_handler and forwards them to the Raft
// - If the buffer size is greater than 10, it sends the messages to the Raft
// - It also sends the messages to the Raft every 5 ms
pub struct Writer {
    // < client's address, client's committed index >
    // shared with publisher
    client_commit_idx: Arc<tokio::sync::Mutex<HashMap<String, u64>>>,
}

// Publisher task
// - It preseves the stream that sended from the client_handler
// - Receives committed messages from the Raft and sends them to the clients
pub struct Publisher {
    state_machine: Arc<tokio::sync::Mutex<Vec<Msg>>>,

    // < client's address, client's committed index >
    // shared with writer
    client_commit_idx: Arc<tokio::sync::Mutex<HashMap<String, u64>>>,

    // < client's address, client stream >
    clients: Arc<tokio::sync::Mutex<HashMap<String, Stream>>>,

    // lock
    pub_lock: Arc<tokio::sync::Mutex<u8>>,
}

impl Publisher {
    pub fn new(
        // when recover the server, backup the state machine from raft.
        state_machine: Vec<Msg>,
        client_commit_idx: Arc<tokio::sync::Mutex<HashMap<String, u64>>>,
    ) -> Self {
        Publisher {
            state_machine: Arc::new(tokio::sync::Mutex::new(state_machine)),
            client_commit_idx,
            clients: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            pub_lock: Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    pub async fn start(
        &self,
        mut commit_rx: Receiver<Entry>,
        mut pub_rx: Receiver<(String, Stream)>,
    ) {
        info!("Publisher started");

        // receive stream from handler and store it in the hashmap
        let clients = self.clients.clone();
        tokio::spawn(async move {
            while let Some((addr, stream)) = pub_rx.recv().await {
                clients.lock().await.insert(addr, stream);
            }
        });

        //[TODO]
        // This is a dummy implementation
        // - It should be refactored to send the messages to the clients
        // - Need some error handling and disconnection handling
        let clients = self.clients.clone();
        let client_commit_idx = self.client_commit_idx.clone();
        let state_machine = self.state_machine.clone();
        let pub_lock = self.pub_lock.clone();
        tokio::spawn(async move {
            while let Some(commit) = commit_rx.recv().await {
                let lock = pub_lock.lock().await;

                // [Warn] type error
                let raft_commit_idx = state_machine.lock().await.len() as u64;
                let c_msg;

                match commit.command {
                    Some(cmd) => {
                        let log_data: LogData = bincode::deserialize(&cmd.data).unwrap();
                        c_msg = Msg::new(
                            cmd.client_id,
                            log_data.get_user_id(),
                            log_data.get_content(),
                            log_data.get_time(),
                            cmd.message_id,
                        );
                    }
                    None => {
                        // no-op
                        c_msg = Msg::new(
                            String::from("raft"),
                            String::from("raft"),
                            String::from("no-op"),
                            Local::now().into(),
                            u64::MAX,
                        );
                    }
                }

                state_machine.lock().await.push(c_msg);
                let mut delete_candidates = Vec::new();

                {
                    let mut clients_: tokio::sync::MutexGuard<
                        '_,
                        HashMap<String, SplitSink<WebSocketStream<TcpStream>, Message>>,
                    > = clients.lock().await;

                    // publish
                    for (addr, client_stream) in clients_.iter_mut() {
                        let client_idx;
                        {
                            let client_commit_idx = client_commit_idx.lock().await;
                            client_idx = client_commit_idx.get(addr).unwrap_or(&0).clone();
                        }

                        // build server msg
                        let mut server_msgs = Vec::new();

                        if client_idx > raft_commit_idx {
                            continue;
                        }

                        for i in client_idx..=raft_commit_idx {
                            let temp =
                                ServerMsg::new(i, state_machine.lock().await[i as usize].clone());
                            server_msgs.push(temp);
                        }

                        info!(
                            "recv from raft & send to {:?} idx: ({:?}): msg len: {:?} raft idx: {:?}",
                            addr,
                            client_idx,
                            server_msgs.len(),
                            raft_commit_idx
                        );

                        let res = client_stream
                            .send(Message::Text(serde_json::to_string(&server_msgs).unwrap()))
                            .await;

                        match res {
                            Ok(_) => {
                                // update client commit index
                                // this information might wrong but client will fix it
                                client_commit_idx
                                    .lock()
                                    .await
                                    .insert(addr.clone(), raft_commit_idx + 1);
                            }
                            Err(_) => {
                                info!("Failed to send to {:?}", addr);
                                delete_candidates.push(addr.clone());
                            }
                        }
                    }
                }

                {
                    let mut clients_: tokio::sync::MutexGuard<
                        '_,
                        HashMap<String, SplitSink<WebSocketStream<TcpStream>, Message>>,
                    > = clients.lock().await;

                    for addr in delete_candidates.iter() {
                        info!("connection closed {:?}", addr);
                        client_commit_idx.lock().await.remove(addr);
                        clients_.remove(addr);
                    }

                    for i in state_machine.lock().await.iter() {
                        debug!("{:?} ", i.get_content());
                    }
                    debug!(" : {:?}", state_machine.lock().await.len());
                }

                drop(lock);
            }
        });

        let clients = self.clients.clone();
        let client_commit_idx = self.client_commit_idx.clone();
        let state_machine = self.state_machine.clone();
        let pub_lock = self.pub_lock.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                let lock = pub_lock.try_lock();
                match lock {
                    Ok(_) => {
                        let raft_commit_idx;
                        if state_machine.lock().await.len() == 0 {
                            drop(lock);
                            continue;
                        } else {
                            raft_commit_idx = state_machine.lock().await.len() as u64 - 1;
                        }

                        let mut delete_candidates = Vec::new();

                        {
                            let mut clients_: tokio::sync::MutexGuard<
                                '_,
                                HashMap<String, SplitSink<WebSocketStream<TcpStream>, Message>>,
                            > = clients.lock().await;

                            // publish
                            for (addr, client_stream) in clients_.iter_mut() {
                                let client_idx;
                                {
                                    let client_commit_idx = client_commit_idx.lock().await;
                                    client_idx = client_commit_idx.get(addr).unwrap_or(&0).clone();
                                }

                                // build server msg
                                let mut server_msgs = Vec::new();

                                if client_idx > raft_commit_idx {
                                    continue;
                                }

                                for i in client_idx..=raft_commit_idx {
                                    let temp = ServerMsg::new(
                                        i,
                                        state_machine.lock().await[i as usize].clone(),
                                    );
                                    server_msgs.push(temp);
                                }

                                info!(
                                    "tick Sending to {:?} cli idx: ({:?}): msg len: {:?} raft idx: {:?}",
                                    addr,
                                    client_idx,
                                    server_msgs.len(),
                                    raft_commit_idx
                                );

                                let res = client_stream
                                    .send(Message::Text(
                                        serde_json::to_string(&server_msgs).unwrap(),
                                    ))
                                    .await;

                                match res {
                                    Ok(_) => {
                                        // update client commit index
                                        // this information might wrong but client will fix it
                                        client_commit_idx
                                            .lock()
                                            .await
                                            .insert(addr.clone(), raft_commit_idx + 1);
                                    }
                                    Err(_) => {
                                        info!("Failed to send to {:?}", addr);
                                        delete_candidates.push(addr.clone());
                                    }
                                }
                            }
                        }

                        {
                            let mut clients_: tokio::sync::MutexGuard<
                                '_,
                                HashMap<String, SplitSink<WebSocketStream<TcpStream>, Message>>,
                            > = clients.lock().await;

                            for addr in delete_candidates.iter() {
                                info!("connection closed {:?}", addr);
                                client_commit_idx.lock().await.remove(addr);
                                clients_.remove(addr);

                                debug!(
                                    "now nums of clients {:?} / {:?}",
                                    client_commit_idx.lock().await.len(),
                                    clients_.len()
                                )
                            }
                        }
                        drop(lock);
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
        });
    }
}

impl Writer {
    pub fn new(client_commit_idx: Arc<tokio::sync::Mutex<HashMap<String, u64>>>) -> Self {
        Writer {
            //buffer: Arc::new(Mutex::new(Vec::new())),
            client_commit_idx,
        }
    }

    pub async fn start(
        &self,
        mut writer_rx: Receiver<(String, ClientMsg)>,
        raft_tx: tokio::sync::mpsc::Sender<UserRequestArgs>,
    ) {
        info!("Writer started");
        let client_commit_idx = self.client_commit_idx.clone();

        // [NOTE]
        // Below code can be refactored to use tokio::select!
        tokio::spawn(async move {
            while let Some((addr, client_msg)) = writer_rx.recv().await {
                info!(
                    "Received a message: {:?}: {:?}",
                    addr,
                    client_msg.get_messages()
                );
                debug!("Recv: {:?}", client_msg);

                let messages: &Vec<Msg> = client_msg.get_messages();
                let index = client_msg.get_committed_index();

                // update client's index
                client_commit_idx.lock().await.insert(addr, index);

                for msg in messages.iter() {
                    info!(
                        "Sending to Raft: {:?} : {:?}",
                        msg.get_id(),
                        msg.get_content()
                    );

                    let log_data = LogData::new(msg.get_id(), msg.get_content(), msg.get_time());

                    let req = UserRequestArgs {
                        client_id: msg.get_id(),
                        message_id: msg.get_time_stamp(),
                        data: bincode::serialize(&log_data).unwrap(),
                    };

                    raft_tx.send(req).await.unwrap();
                }
            }
        });
    }
}
