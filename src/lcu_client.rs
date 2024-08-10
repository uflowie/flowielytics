use base64::{engine::general_purpose, Engine};
use futures::SinkExt;
use native_tls::TlsConnector;
use serde::{de::IgnoredAny, Deserialize};
use sysinfo::System;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

#[derive(Clone, Debug)]
pub enum LCUState {
    NotConnected,
    Connected,
    Playing { champion: String, game_mode: String },
}

pub async fn run_lcu_client() -> watch::Receiver<LCUState> {
    let mut last_champ = "".to_string();
    let mut last_game_mode = "".to_string();

    let (tx, rx) = watch::channel(LCUState::NotConnected);

    tokio::spawn(async move {
        let mut sys = System::new_all();

        loop {
            let (token, port) = get_lcu_process_data(&mut sys).await;

            let token = general_purpose::STANDARD.encode(format!("riot:{token}"));
            let auth = format!("Basic {}", token);

            println!("auth token: {}", auth);

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
                .expect("Failed to send initial message to websocket");

            // wait on websocket for new events
            while let Some(Ok(msg)) = socket.next().await {
                if let tungstenite::Message::Text(msg) = msg {
                    println!("Received message: {}\n", msg);

                    // parse msg
                    if let Ok(LCUEvent(_, _, LCUEventData { data: msg })) =
                        serde_json::from_str::<LCUEvent>(&msg)
                    {
                        match msg {
                            LCUResource::Summoner(summoner) => {
                                let champion_name = summoner.champion_name;
                                if !summoner.is_self
                                    || champion_name == ""
                                    || champion_name == last_champ
                                {
                                    continue;
                                }
                                last_champ = champion_name;
                                if last_game_mode != "" {
                                    tx.send(LCUState::Playing {
                                        champion: last_champ.clone(),
                                        game_mode: last_game_mode.clone(),
                                    })
                                    .unwrap();
                                }
                            }
                            LCUResource::Gameflow(gameflow) => {
                                let queue_name = gameflow.game_data.queue.game_mode;
                                if queue_name == last_game_mode || queue_name == "" {
                                    continue;
                                }
                                last_game_mode = queue_name;
                                if last_champ != "" {
                                    tx.send(LCUState::Playing {
                                        champion: last_champ.clone(),
                                        game_mode: last_game_mode.clone(),
                                    })
                                    .unwrap();
                                }
                            }
                            LCUResource::Other(_) => {}
                        }
                    }
                }
            }
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

#[derive(Deserialize)]
struct LCUEvent(i32, String, LCUEventData);

#[derive(Deserialize)]
struct LCUEventData {
    data: LCUResource,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LCUResource {
    Summoner(Summoner),
    Gameflow(Gameflow),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summoner {
    pub is_self: bool,
    pub champion_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gameflow {
    pub game_data: GameData,
}

#[derive(Deserialize)]
struct GameData {
    pub queue: Queue,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Queue {
    pub game_mode: String,
}
