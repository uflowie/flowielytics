use std::collections::HashMap;

pub struct Lolalytics;

pub trait StatisticsUrlProducer {
    fn get_url(champion: &str, game_mode: &str, params: &HashMap<String, String>) -> String;
}

impl StatisticsUrlProducer for Lolalytics {
    fn get_url(champion: &str, game_mode: &str, params: &HashMap<String, String>) -> String {
        let champion = match champion {
            "Dr. Mundo" => "drmundo",
            "Renata Glasc" => "renata",
            "Nunu & Willump" => "nunu",
            _ => champion,
        }
        .replace(" ", "")
        .to_lowercase();

        let binding = "30".to_string();
        let patch = params.get("patch").unwrap_or(&binding);

        let game_mode = game_mode.replace(" ", "").to_lowercase();

        format!("https://lolalytics.com/lol/{champion}/{game_mode}/build/?patch={patch}")
    }
}
