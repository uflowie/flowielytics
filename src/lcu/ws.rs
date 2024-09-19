use anyhow::Result;
use base64::{engine::general_purpose, Engine};
use futures::{SinkExt, Stream};
use native_tls::TlsConnector;
use tokio_stream::StreamExt;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use crate::lcu::models::{LCUEvent, LCUResource};

pub enum PlayingEvent {
    ChampionName(String),
    GameModeName(String),
}

pub async fn get_event_stream(token: &str, port: &str) -> Result<impl Stream<Item = PlayingEvent>> {
    let token = general_purpose::STANDARD.encode(format!("riot:{}", token));
    let auth = format!("Basic {}", token);

    println!("auth token: {}", auth);

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
