use base64::{engine::general_purpose, Engine};
use futures::{SinkExt, Stream};
use native_tls::TlsConnector;
use serde::{de::IgnoredAny, Deserialize};
use sysinfo::System;
use tokio::{net::TcpStream, sync::watch};
use tokio_stream::StreamExt;
use tokio_tungstenite::{
    tungstenite::{self, client::IntoClientRequest, Error},
    MaybeTlsStream, WebSocketStream,
};

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
            tx.send(LCUState::NotConnected).unwrap();

            let (token, port) = get_token_and_port(&mut sys).await;

            let socket = match get_ws(token, port).await {
                Ok(socket) => socket,
                Err(e) => {
                    println!("Failed to connect to websocket: {}", e);
                    continue;
                }
            };

            let mut event_stream = match get_resource_stream(socket).await {
                Ok(event_stream) => event_stream,
                Err(e) => {
                    println!("Failed to get event stream: {}", e);
                    tx.send(LCUState::NotConnected).unwrap();
                    continue;
                }
            };

            tx.send(LCUState::Connected).unwrap();

            while let Some(resource) = event_stream.next().await {
                match resource {
                    LCUResource::Summoner(summoner) => {
                        let champion_name = summoner.champion_name;
                        if !summoner.is_self || champion_name == "" || champion_name == last_champ {
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
                        let game_mode = gameflow.game_data.queue.game_mode;
                        if game_mode == last_game_mode || game_mode == "" {
                            continue;
                        }
                        last_game_mode = game_mode;
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
    });

    rx
}

async fn get_token_and_port(sys: &mut System) -> (String, String) {
    loop {
        sys.refresh_all();

        let mut token = None;
        let mut port = None;

        for process in sys.processes_by_name("LeagueClientUx.exe") {
            for arg in process.cmd() {
                if let Some(t) = arg.strip_prefix("--remoting-auth-token=") {
                    token = Some(t.to_string());
                }
                if let Some(p) = arg.strip_prefix("--app-port=") {
                    port = Some(p.to_string());
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

async fn get_ws(
    token: String,
    port: String,
) -> anyhow::Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let token = general_purpose::STANDARD.encode(format!("riot:{token}"));
    let auth = format!("Basic {}", token);

    println!("auth token: {}", auth);

    let ws_addr = format!("wss://127.0.0.1:{port}");
    let mut request = ws_addr.into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("Authorization", auth.parse()?);

    let config = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    // create tls connector
    let connector = tokio_tungstenite::Connector::NativeTls(config);

    let (socket, _) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await?;

    Ok(socket)
}

async fn get_resource_stream(
    mut socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> Result<impl Stream<Item = LCUResource>, Error> {
    // subscribe to json api events
    socket
        .send(tungstenite::Message::Text(
            "[5, \"OnJsonApiEvent\"]".to_string(),
        ))
        .await?;

    Ok(socket.filter_map(|msg| match msg {
        Ok(tungstenite::Message::Text(msg)) => serde_json::from_str::<LCUEvent>(&msg)
            .ok()
            .map(|event| event.2.data),
        _ => None,
    }))
}

#[derive(Deserialize)]
struct LCUEvent(#[allow(unused)] i32, #[allow(unused)] String, LCUEventData);

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
    is_self: bool,
    champion_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gameflow {
    game_data: GameData,
}

#[derive(Deserialize)]
struct GameData {
    queue: Queue,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Queue {
    game_mode: String,
}
