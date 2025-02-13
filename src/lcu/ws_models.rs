use serde::{de::IgnoredAny, Deserialize};

#[derive(Deserialize)]
pub struct LCUEvent(pub i32, pub String, pub LCUEventData);

#[derive(Deserialize)]
pub struct LCUEventData {
    pub data: LCUResource,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum LCUResource {
    Summoner(Summoner),
    Gameflow(Gameflow),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summoner {
    pub is_self: bool,
    pub champion_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gameflow {
    pub game_data: GameData,
}

#[derive(Deserialize)]
pub struct GameData {
    pub queue: Queue,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub game_mode: String,
}
