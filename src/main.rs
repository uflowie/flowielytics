mod lcu_client;
mod statistics_providers;
mod templates;

use std::convert::Infallible;

use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use futures::stream::Stream;
use lcu_client::{run_lcu_client, LCUState};
use statistics_providers::{Lolalytics, StatisticsUrlProducer};
use templates::{ConnectedTemplate, NotConnectedTemplate, PlayingTemplate, SseTemplate};
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

async fn sse_handler<T: StatisticsUrlProducer>(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = tokio_stream::wrappers::WatchStream::new(state.lcu_state_rx).map(|state| {
        let data = match state {
            LCUState::NotConnected => NotConnectedTemplate.render_sse().unwrap(),
            LCUState::Connected => ConnectedTemplate.render_sse().unwrap(),
            LCUState::Playing {
                champion,
                game_mode,
            } => PlayingTemplate {
                url: T::get_url(&champion, &game_mode),
            }
            .render_sse()
            .unwrap(),
        };

        Ok(Event::default().data(data))
    });

    Sse::new(stream)
}
