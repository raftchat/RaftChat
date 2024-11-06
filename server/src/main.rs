use axum::Extension;
use axum::{routing::get, Router};
use futures_util::stream::SplitSink;
use log::info;
use raft::database;
use raft::mock_raft;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use tower_http::services::ServeDir;

mod axum_handler;
mod data_model;
mod events;

#[derive(Clone, serde::Serialize, Debug)]
struct Config {
    peers: Vec<String>, // [TODO] change to Vec<&str>
    port: u16,
    socket_port: u16,
}

async fn setup() -> Config {
    // log setting
    log4rs::init_file("../config/log4rs.yaml", Default::default()).unwrap();

    // config setting
    dotenv::from_path("../config/config.env").unwrap();

    let peers: Vec<String> = env::var("PEER")
        .unwrap()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let port: u16 = env::var("WEB_PORT")
        .ok()
        .and_then(|val| val.parse::<u16>().ok())
        .unwrap_or(3001);

    let socket_port: u16 = env::var("SOCKET_PORT")
        .ok()
        .and_then(|val| val.parse::<u16>().ok())
        .unwrap_or(9001);

    return Config {
        peers: peers,
        port: port,
        socket_port: socket_port,
    };
}

async fn run_axum(config: &Config) {
    // axum server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = Router::new()
        .route("/", get(axum_handler::handler))
        .nest_service("/static", ServeDir::new("../client/static"))
        .route("/get_info", get(axum_handler::get_info))
        .layer(Extension(config.clone()));

    let listener = TcpListener::bind(&addr).await.unwrap();

    tokio::spawn(async move {
        info!("AXUM listening on {}", addr);
        axum::serve(listener, app).await.unwrap();
    });
}

// - make channel from Database like raft or sync db
// - start server read write tasks
async fn run_tasks<T: database::DB>(
    db: T,
    hash: Arc<tokio::sync::Mutex<HashMap<String, u64>>>,
    config: &Config,
) -> (
    Sender<(String, data_model::msg::ClientMsg)>,
    Sender<(String, SplitSink<WebSocketStream<TcpStream>, Message>)>,
) {
    let (commit_rx, database_tx) = db.make_channel(1, config.peers.clone());

    // writer task
    let (writer_tx, writer_rx): (
        Sender<(String, data_model::msg::ClientMsg)>,
        Receiver<(String, data_model::msg::ClientMsg)>,
    ) = mpsc::channel(15);

    let writer = events::task::Writer::new(hash.clone());
    writer.start(writer_rx, database_tx).await;

    // publisher task
    let (pub_tx, pub_rx): (
        Sender<(String, SplitSink<WebSocketStream<TcpStream>, Message>)>,
        Receiver<(String, SplitSink<WebSocketStream<TcpStream>, Message>)>,
    ) = mpsc::channel(15);

    let publisher = events::task::Publisher::new(Vec::new(), hash.clone());
    publisher.start(commit_rx, pub_rx).await;

    return (writer_tx, pub_tx);
}

#[tokio::main]
async fn main() {
    let config: Config = setup().await;

    info!("{:?}", config);

    run_axum(&config).await;

    // writer and publisher set up
    let client_commit_idx: Arc<tokio::sync::Mutex<HashMap<String, u64>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let (writer_tx, pub_tx) = run_tasks(mock_raft::Raft {}, client_commit_idx, &config).await;

    // websocket server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.socket_port));
    let server = TcpListener::bind(addr).await;
    let listener = server.expect("failed to bind");

    while let Ok((stream, addr)) = listener.accept().await {
        let w_tx = writer_tx.clone();
        let p_tx = pub_tx.clone();
        tokio::spawn(async move {
            events::handler::client_handler(stream, addr, w_tx, p_tx).await;
        });
    }
}
