mod lcu_client;

use std::convert::Infallible;

use askama::Template;
use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use futures::stream::Stream;
use lcu_client::{run_lcu_client, LCUState};
use tokio::sync::watch::{self};
use tokio::{self};
use tokio_stream::StreamExt;
use tower_http::services::ServeFile;

#[tokio::main]
async fn main() {
    let lcu_state_rx = run_lcu_client().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    let app = Router::new()
        .route("/sse", get(sse_handler::<Lolalytics>))
        .route_service("/", ServeFile::new("assets/index.html"))
        .with_state(AppState { lcu_state_rx });

    // serve
    axum::serve(listener, app).await.unwrap();
}

#[derive(Clone)]
struct AppState {
    lcu_state_rx: watch::Receiver<LCUState>,
}

struct Lolalytics;

impl UrlProducer for Lolalytics {
    fn get_url(champion: &str, game_mode: &str) -> String {
        let champion = match champion {
            "Dr. Mundo" => "drmundo",
            "Renata Glasc" => "renata",
            "Nunu & Willump" => "nunu",
            _ => champion,
        }
        .replace(" ", "")
        .to_lowercase();
        let game_mode = game_mode.replace(" ", "").to_lowercase();
        format!("https://lolalytics.com/lol/{champion}/{game_mode}/build/?patch=30")
    }
}

trait UrlProducer {
    fn get_url(champion: &str, game_mode: &str) -> String;
}

async fn sse_handler<T: UrlProducer>(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = tokio_stream::wrappers::WatchStream::new(state.lcu_state_rx).map(|state| {
        let data = match state {
            LCUState::NotConnected => NotConnectedTemplate.render_sse().unwrap(),
            LCUState::Connected => ConnectedTemplate.render_sse().unwrap(),
            LCUState::Playing {
                champion,
                game_mode,
            } => {
                let url = T::get_url(&champion, &game_mode);
                format!("<iframe src=\"{url}\" class=\"h-screen w-full aspect-auto\"></iframe>")
            }
        };

        Ok(Event::default().data(data))
    });

    Sse::new(stream)
}

#[derive(Template)]
#[template(path = "not-connected.html")]
struct NotConnectedTemplate;

#[derive(Template)]
#[template(path = "connected.html")]
struct ConnectedTemplate;

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
