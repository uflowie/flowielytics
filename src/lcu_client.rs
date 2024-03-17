use serde::{de::IgnoredAny, Deserialize};

pub fn parse_lcu_ws_message(msg: &str) -> Option<LCUResource> {
    Some(serde_json::from_str::<LCUEvent>(msg).ok()?.2.data)
}

#[derive(Deserialize)]
struct LCUEvent(i32, String, LCUEventData);

#[derive(Deserialize)]
struct LCUEventData {
    data: LCUResource,
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
