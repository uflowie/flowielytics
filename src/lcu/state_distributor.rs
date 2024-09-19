use super::state::LCUState;

pub struct StateDistributor {
    last_champion_name: Option<String>,
    last_game_mode_name: Option<String>,
    tx: tokio::sync::watch::Sender<LCUState>,
}

impl StateDistributor {
    pub fn new(tx: tokio::sync::watch::Sender<LCUState>) -> StateDistributor {
        Self {
            last_champion_name: None,
            last_game_mode_name: None,
            tx,
        }
    }

    pub fn update_champion_name(&mut self, champion_name: String) {
        if Some(&champion_name) != self.last_champion_name.as_ref() {
            self.last_champion_name = Some(champion_name);
            self.update_state();
        }
    }

    pub fn update_game_mode_name(&mut self, game_mode_name: String) {
        if Some(&game_mode_name) != self.last_game_mode_name.as_ref() {
            self.last_game_mode_name = Some(game_mode_name);
            self.update_state();
        }
    }

    fn update_state(&mut self) {
        if let (Some(champion_name), Some(game_mode_name)) =
            (&self.last_champion_name, &self.last_game_mode_name)
        {
            self.tx
                .send(LCUState::Playing {
                    champion: champion_name.clone(),
                    game_mode: game_mode_name.clone(),
                })
                .unwrap();
        }
    }
}