use axum::{
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use futures::{
    stream::Stream,
    SinkExt,
};
use native_tls::TlsConnector;
use serde_json::Value;
use std::{convert::Infallible, time::Duration};
use sysinfo::System;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::{self, select};
use tokio_stream::StreamExt;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[tokio::main]
async fn main() {
    let lcu_state_rx = run_test_provider().await;
    let sub_tx = run_distributor(lcu_state_rx).await;
    let _ = run_provider().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    let app = Router::new().route("/sse", get(move || sse_handler(sub_tx.clone())));

    // serve
    axum::serve(listener, app).await.unwrap();
}

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
            LCUState::NotConnected => "Not connected".to_string(),
            LCUState::Connected => "Connected".to_string(),
            LCUState::Playing {
                champion,
                game_mode,
            } => format!("Playing {} in {}", champion, game_mode),
        };
        Ok(Event::default().data(data))
    });

    Sse::new(stream)
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

            if token.is_none() || port.is_none() {
                println!("Couldn't find auth token, sleeping for 1 second");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            let token = token.unwrap();
            let port = port.unwrap();
            let token = format!("riot:{token}");

            // base64 encode riot:token
            let token = general_purpose::STANDARD.encode(&token);
            let auth = format!("Basic {}", token);

            let ws_addr = format!("wss://127.0.0.1:{port}");
            let mut request = ws_addr.into_client_request().unwrap();
            let headers = request.headers_mut();
            headers.insert("Authorization", auth.parse().unwrap());

            let config = TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap();

            // create tls connector
            let mut connector = tokio_tungstenite::Connector::NativeTls(config);

            let (mut socket, _) = tokio_tungstenite::connect_async_tls_with_config(
                request,
                None,
                false,
                Some(connector),
            )
            .await
            .expect("Failed to connect to websocket");

            // connect to websocket using token
            // let (mut socket, _) = tokio_tungstenite::connect_async(request)
            //     .await
            //     .expect("Failed to connect to websocket");

            socket
                .send(tungstenite::Message::Text(
                    "[5, \"OnJsonApiEvent\"]".to_string(),
                ))
                .await
                .expect("Failed to send message to websocket");

            // wait on websocket for new events
            while let Some(Ok(msg)) = socket.next().await {
                if let tungstenite::Message::Text(msg) = msg {
                    // println!("Received message: {}", msg);
                    println!();

                    if let Some(uri) = get_uri_from_lcu_event(&msg) {
                        if uri.starts_with("/lol-gameflow/v1/session") {
                            println!("Received message: {}", msg);
                        println!("Received uri: {}", uri);
                            
                        }
                    }
                }
            }
        }
    });

    rx
}

fn get_uri_from_lcu_event(msg: &str) -> Option<String> {
    let value: Value = serde_json::from_str(msg).ok()?;
    value.get(2)?.get("uri")?.as_str().map(|s| s.to_string())
}

// fn parse_champ_data_from_msg(msg: &str) -> Option<(String, String)> {
//     let value: Option<Value> = serde_json::from_str(msg).ok()?;
//     let uri = value?.get(3)?.get("uri")?.as_str()?;
//     let uri = uri.split("/").collect::<Vec<&str>>();
//     let champ = uri.get(2)?.to_string();
//     let game_mode = uri.get(3)?.to_string();
//     Some((champ, game_mode))
// }

#[derive(Clone, Debug)]
enum LCUState {
    NotConnected,
    Connected,
    Playing { champion: String, game_mode: String },
}
