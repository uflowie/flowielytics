mod lcu_client;

use askama::Template;
use axum::{
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use futures::{stream::Stream, SinkExt};
use native_tls::TlsConnector;
use serde_json::Value;
use std::{convert::Infallible, time::Duration};
use sysinfo::System;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::{self, select};
use tokio_stream::StreamExt;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tower_http::services::ServeFile;

#[tokio::main]
async fn main() {
    let lcu_state_rx = run_test_provider().await;
    let sub_tx = run_distributor(lcu_state_rx).await;
    let _ = run_provider().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    let app = Router::new()
        .route("/sse", get(move || sse_handler(sub_tx.clone())))
        .route_service("/", ServeFile::new("assets/index.html"));

    // serve
    axum::serve(listener, app).await.unwrap();
}

#[derive(Template)]
#[template(path = "not-connected.html")]
struct NotConnectedTemplate;

#[derive(Template)]
#[template(path = "connected.html")]
struct ConnectedTemplate;

async fn sse_handler(
    sub_tx: Sender<Sender<LCUState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel(100);

    sub_tx
        .send(tx)
        .await
        .expect("Failed to subscribe to state changes");

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|state| {
        let data = match state {
            LCUState::NotConnected => NotConnectedTemplate.render_sse().unwrap(),
            LCUState::Connected => ConnectedTemplate.render_sse().unwrap(),
            LCUState::Playing {
                champion,
                game_mode,
            } => get_iframe(&champion, &game_mode),
        };

        Ok(Event::default().data(data))
    });

    Sse::new(stream)
}

fn get_iframe(champion: &str, game_mode: &str) -> String {
    let champion = match champion {
        "Dr. Mundo" => "drmundo",
        "Renata Glasc" => "renata",
        "Nunu & Willump" => "nunu",
        _ => champion,
    }
    .replace(" ", "")
    .to_lowercase();
    let game_mode = game_mode.replace(" ", "").to_lowercase();
    format!("<iframe src=\"https://lolalytics.com/lol/{champion}/{game_mode}/build/?patch=30\" class=\"h-screen w-full aspect-auto hidden\"></iframe>")
}

trait SseTemplate {
    fn render_sse(&self) -> askama::Result<String>;
}

impl<T> SseTemplate for T
where
    T: Template,
{
    fn render_sse(&self) -> askama::Result<String> {
        self.render()
            .map(|x| x.chars().filter(|&c| c != '\n' && c != '\r').collect())
    }
}

// distributes the state of the lcu to all its subscribers. takes a receiver that listens for state changes
// and returns a sender that can be used to subscribe to state changes
async fn run_distributor(mut lcu_state_receiver: Receiver<LCUState>) -> Sender<Sender<LCUState>> {
    let (tx, mut rx) = mpsc::channel::<Sender<LCUState>>(100);
    let mut connections: Vec<Sender<LCUState>> = Vec::new();
    let mut state = LCUState::NotConnected;

    tokio::spawn(async move {
        loop {
            select! {
                new_subscriber = rx.recv() => {
                    if let Some(sub) = new_subscriber {
                        println!("Received new connection");
                        // Attempt to send initial state, if fail, do not add to connections
                        if sub.send(state.clone()).await.is_ok() {
                            connections.push(sub);
                        } else {
                            println!("Failed to send initial state, not adding to connections");
                        }
                    }
                },
                new_state = lcu_state_receiver.recv() => {
                    state = new_state.expect("Failed to receive from channel");
                    let mut failed_indices = Vec::new();

                    for (index, conn) in connections.iter().enumerate() {
                        if conn.send(state.clone()).await.is_err() {
                            failed_indices.push(index);
                        }
                    }

                    // Remove unreachable subscribers in reverse order to avoid shifting indices
                    for index in failed_indices.iter().rev() {
                        println!("Removing connection");
                        connections.swap_remove(*index);
                    }
                }
            }
        }
    });
    tx
}

async fn run_test_provider() -> Receiver<LCUState> {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            tx.send(LCUState::Connected)
                .await
                .expect("Failed to send state from LCU-Connector");
            tokio::time::sleep(Duration::from_secs(1)).await;
            tx.send(LCUState::Playing {
                champion: "Yasuo".to_string(),
                game_mode: "Ranked".to_string(),
            })
            .await
            .expect("Failed to send state from LCU-Connector");
            tokio::time::sleep(Duration::from_secs(1)).await;
            tx.send(LCUState::NotConnected)
                .await
                .expect("Failed to send state from LCU-Connector");
        }
    });

    rx
}

async fn get_lcu_process_data(sys: &mut System) -> (String, String) {
    // get lcu port and remoting token
    loop {
        sys.refresh_all();

        let mut token = None;
        let mut port = None;

        for (_, process) in sys.processes() {
            if process.name() != "LeagueClientUx.exe" {
                continue;
            }

            for arg in process.cmd() {
                if arg.starts_with("--remoting-auth-token") {
                    println!("Found auth token: {}", arg);
                    token = Some(arg.split("=").collect::<Vec<&str>>()[1].to_string());
                }
                if arg.starts_with("--app-port") {
                    println!("Found app port: {}", arg);
                    port = Some(arg.split("=").collect::<Vec<&str>>()[1].to_string());
                }
            }
        }

        if let (Some(token), Some(port)) = (token, port) {
            return (token, port);
        }

        println!("Couldn't find auth token, sleeping for 1 second");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

// this function is responsible for connecting to the lcu websocket and sending its state whenever it changes
// returns a receiver that can be used to listen for state changes
async fn run_provider() -> Receiver<LCUState> {
    // open questions: if the connection is lost, do we tell the receiver right away? i feel like i really only
    // want to know if i can tell for sure that the league client has been closed and maybe not even then? would be
    // more of a visual debugging thing at the end of the day. there is a case for only showing the disconnected/connected states
    // when no games have been played since the program started. if a game has been played before, the most practical behavior is
    // to just display that champ-game pair until the program is shut down

    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        let mut sys = System::new_all();

        loop {
            let (token, port) = get_lcu_process_data(&mut sys).await;

            let token = general_purpose::STANDARD.encode(&token);
            let auth = format!("Basic {}", format!("riot:{token}"));

            let ws_addr = format!("wss://127.0.0.1:{port}");
            let mut request = ws_addr.into_client_request().unwrap();
            let headers = request.headers_mut();
            headers.insert("Authorization", auth.parse().unwrap());

            let config = TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap();

            // create tls connector
            let connector = tokio_tungstenite::Connector::NativeTls(config);

            let (mut socket, _) = tokio_tungstenite::connect_async_tls_with_config(
                request,
                None,
                false,
                Some(connector),
            )
            .await
            .expect("Failed to connect to websocket");

            // subscribe to json api events
            socket
                .send(tungstenite::Message::Text(
                    "[5, \"OnJsonApiEvent\"]".to_string(),
                ))
                .await
                .expect("Failed to send message to websocket");

            // wait on websocket for new events
            while let Some(Ok(msg)) = socket.next().await {
                if let tungstenite::Message::Text(msg) = msg {
                    println!("Received message: {}\n", msg);

                    // parse msg
                    if let Some(msg) = lcu_client::parse_lcu_ws_message(&msg) {
                        match msg {
                            lcu_client::LCUResource::Summoner(summoner) => {
                                if summoner.is_self {
                                    println!("Found summoner: {}", summoner.champion_name);
                                }
                            }
                            lcu_client::LCUResource::Gameflow(gameflow) => {
                                println!("Found gameflow: {}", gameflow.game_data.queue.game_mode);
                            }
                            lcu_client::LCUResource::Other(_) => {}
                        }
                    }
                }
            }
        }
    });

    rx
}

#[derive(Clone, Debug)]
enum LCUState {
    NotConnected,
    Connected,
    Playing { champion: String, game_mode: String },
}
