use super::state::StateProvider;

pub struct RestClient {

}

impl StateProvider for RestClient {
    async fn try_get_champion_name(&mut self) -> Option<String> {
        None
    }

    async fn try_get_game_mode_name(&mut self) -> Option<String> {
        None
    }
}