use super::state::StateProvider;

pub struct InGameClient {

}

impl StateProvider for InGameClient {
    async fn try_get_champion_name(&mut self) -> Option<String> {
        None
    }

    async fn try_get_game_mode_name(&mut self) -> Option<String> {
        None
    }
}