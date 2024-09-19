#[derive(Clone, Debug)]
pub enum LCUState {
    NotConnected,
    Playing { champion: String, game_mode: String },
}

pub trait StateProvider {
    async fn try_get_champion_name(&mut self) -> Option<String>;

    async fn try_get_game_mode_name(&mut self) -> Option<String>;
}
