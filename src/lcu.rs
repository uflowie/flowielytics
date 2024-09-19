use ingame::InGameClient;
use process_info_provider::ProcessInfoProvider;
use rest::RestClient;
use state::{LCUState, StateProvider};
use state_distributor::StateDistributor;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use ws::get_event_stream;

mod ingame;
mod models;
mod process_info_provider;
mod rest;
pub mod state;
mod state_distributor;
mod ws;

pub async fn get_state_rx() -> watch::Receiver<LCUState> {
    let (tx, rx) = watch::channel(LCUState::NotConnected);
    let mut distributor = StateDistributor::new(tx);
    let mut process_info_provider = ProcessInfoProvider::new();

    tokio::spawn(async move {
        loop {
            let (token, port) = process_info_provider.get_token_and_port().await;

            let mut rest_client = RestClient {};
            try_get_state_from_provider(&mut rest_client, &mut distributor).await;

            let mut ingame_client = InGameClient {};
            try_get_state_from_provider(&mut ingame_client, &mut distributor).await;

            if let Ok(mut stream) = get_event_stream(&token, &port).await {
                while let Some(event) = stream.next().await {
                    match event {
                        ws::PlayingEvent::ChampionName(name) => {
                            distributor.update_champion_name(name)
                        }
                        ws::PlayingEvent::GameModeName(name) => {
                            distributor.update_game_mode_name(name)
                        }
                    }
                }
            }
        }
    });

    rx
}

async fn try_get_state_from_provider<T: StateProvider>(
    provider: &mut T,
    distributor: &mut StateDistributor,
) {
    if let Some(champion_name) = provider.try_get_champion_name().await {
        distributor.update_champion_name(champion_name);
    }

    if let Some(game_mode_name) = provider.try_get_game_mode_name().await {
        distributor.update_game_mode_name(game_mode_name);
    }
}
