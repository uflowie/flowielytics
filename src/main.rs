mod lcu;
mod statistics_providers;
mod templates;

use std::{collections::HashMap, convert::Infallible, net::SocketAddr};

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{sse::{Event, Sse}, Html, IntoResponse},
    routing::get,
    Router,
};
use clap::{arg, command, Parser};
use futures::stream::Stream;
use lcu::state::LCUState;
use statistics_providers::{Lolalytics, StatisticsUrlProducer};
use templates::{IndexTemplate, NotConnectedTemplate, PlayingTemplate, SseTemplate};
use tokio::sync::watch::{self};
use tokio::{self};
use tokio_stream::StreamExt;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 3000)]
    port: u16,

    #[arg(short = 'n', long = "no-browser")]
    no_browser: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    
    let lcu_state_rx = lcu::get_state_rx().await;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap();

    let app = Router::new()
        .route("/sse/lolalytics", get(sse_handler::<Lolalytics>))
        .route("/:site_name", get(index_handler))
        .with_state(AppState { lcu_state_rx });

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    if !args.no_browser {
        let url = format!("http://{}/lolalytics", addr);
        open::that(url).unwrap();
    }

    handle.await.unwrap();
}

#[derive(Clone)]
struct AppState {
    lcu_state_rx: watch::Receiver<LCUState>,
}

async fn index_handler(Path(url): Path<String>, Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    let response = IndexTemplate {
        site_name: url,
        query_string: params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<String>>()
            .join("&"),
    }
    .render()
    .unwrap();

    (axum::http::StatusCode::OK, Html(response).into_response())
}

async fn sse_handler<T: StatisticsUrlProducer>(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = tokio_stream::wrappers::WatchStream::new(state.lcu_state_rx).map(move |state| {
        let data = match state {
            LCUState::NotConnected => NotConnectedTemplate.render_sse().unwrap(),
            LCUState::Playing {
                champion,
                game_mode,
            } => PlayingTemplate {
                url: T::get_url(&champion, &game_mode, &params),
            }
            .render_sse()
            .unwrap(),
        };

        Ok(Event::default().data(data))
    });

    Sse::new(stream)
}
