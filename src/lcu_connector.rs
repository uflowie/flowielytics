use anyhow::Result;
use base64::{engine::general_purpose, Engine};
use futures::{future::join_all, SinkExt, Stream};
use http::{HeaderMap, HeaderValue};
use native_tls::TlsConnector;
use serde::{de::IgnoredAny, Deserialize};
use sysinfo::System;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

#[derive(Clone, Eq, PartialEq)]
pub struct GameState {
    pub champion_name: String,
    pub game_mode_name: String,
}

pub async fn get_state_rx() -> watch::Receiver<Option<GameState>> {
    let (tx, rx) = watch::channel(None);
    let mut sys = System::new_all();
    let mut tracker = GameStateTracker {
        tx,
        champion_name: None,
        game_mode_name: None,
    };

    tokio::spawn(async move {
        loop {
            let Some((token, port)) = get_token_and_port(&mut sys).await else {
                continue;
            };

            if let Some(game_state) = try_get_current_game_state(&token, &port).await {
                tracker.update_game_state(game_state);
            }

            if let Ok(mut stream) = get_event_stream(&token, &port).await {
                while let Some(event) = stream.next().await {
                    match event {
                        PlayingEvent::ChampionName(name) => tracker.update_champion_name(name),
                        PlayingEvent::GameModeName(name) => tracker.update_game_mode_name(name),
                    }
                }
            }
        }
    });

    rx
}

struct GameStateTracker {
    champion_name: Option<String>,
    game_mode_name: Option<String>,
    tx: watch::Sender<Option<GameState>>,
}

impl GameStateTracker {
    fn update_champion_name(&mut self, name: String) {
        if name.is_empty() {
            return;
        }

        self.champion_name = Some(name);
        self.notify_if_modified();
    }

    fn update_game_mode_name(&mut self, name: String) {
        if name.is_empty() {
            return;
        }

        self.game_mode_name = Some(name);
        self.notify_if_modified();
    }

    fn update_game_state(&mut self, state: GameState) {
        if state.champion_name.is_empty() || state.game_mode_name.is_empty() {
            return;
        }

        self.champion_name = Some(state.champion_name);
        self.game_mode_name = Some(state.game_mode_name);
        self.notify_if_modified();
    }

    fn notify_if_modified(&mut self) {
        if let (Some(champion_name), Some(game_mode_name)) =
            (&self.champion_name, &self.game_mode_name)
        {
            self.tx
                .send(Some(GameState {
                    champion_name: champion_name.clone(),
                    game_mode_name: game_mode_name.clone(),
                }))
                .unwrap();
        }
    }
}

async fn get_token_and_port(sys: &mut System) -> Option<(String, String)> {
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
            println!("port: {}", port);
            let token = general_purpose::STANDARD.encode(format!("riot:{}", token));
            return Some((token, port));
        }

        println!("Couldn't find auth token, sleeping for 1 second");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn try_get_current_game_state(token: &str, port: &str) -> Option<GameState> {
    let mut champion_name = None;

    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Basic {}", token)).unwrap(),
    );

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .build()
        .unwrap();

    let tasks = (0..10).map(|id| {
        let url = format!("https://127.0.0.1:{port}/lol-champ-select/v1/summoners/{id}");
        let client = &client;
        async move {
            let response = client.get(&url).send().await?;
            let summoner = response.json::<Summoner>().await?;
            anyhow::Result::Ok(summoner)
        }
    });

    let results: Vec<anyhow::Result<_>> = join_all(tasks).await;

    for result in results.into_iter().flatten() {
        if result.is_self {
            champion_name = Some(result.champion_name);
        }
    }

    let game_mode_name = client
        .get(&format!(
            "https://127.0.0.1:{}/lol-gameflow/v1/session",
            port
        ))
        .send()
        .await
        .ok()?
        .json::<Gameflow>()
        .await
        .ok()?
        .game_data
        .queue
        .game_mode;

    if let Some(champion_name) = champion_name {
        Some(GameState {
            champion_name,
            game_mode_name,
        })
    } else {
        None
    }
}

enum PlayingEvent {
    ChampionName(String),
    GameModeName(String),
}

async fn get_event_stream(token: &str, port: &str) -> Result<impl Stream<Item = PlayingEvent>> {
    let auth = format!("Basic {}", token);

    let ws_addr = format!("wss://127.0.0.1:{}", port);
    let mut request = ws_addr.into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("Authorization", auth.parse()?);

    let config = TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    // create tls connector
    let connector = tokio_tungstenite::Connector::NativeTls(config);

    let (mut socket, _) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await?;

    socket
        .send(tungstenite::Message::Text(
            "[5, \"OnJsonApiEvent\"]".to_string(),
        ))
        .await?;

    let stream = socket
        .filter_map(|msg| match msg {
            Ok(tungstenite::Message::Text(msg)) => serde_json::from_str::<LCUEvent>(&msg)
                .ok()
                .map(|event| event.2.data),
            _ => None,
        })
        .filter_map(|resource| match resource {
            LCUResource::Summoner(summoner) => {
                let champion_name = summoner.champion_name;
                if !summoner.is_self || champion_name.is_empty() {
                    None
                } else {
                    Some(PlayingEvent::ChampionName(champion_name))
                }
            }
            LCUResource::Gameflow(gameflow) => {
                let game_mode = gameflow.game_data.queue.game_mode;
                if game_mode.is_empty() {
                    None
                } else {
                    Some(PlayingEvent::GameModeName(game_mode))
                }
            }
            _ => None,
        });

    Ok(stream)
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
