use std::collections::HashMap;

pub trait StatisticsUrlProducer {
    fn get_url(champion: &str, game_mode: &str, params: &HashMap<String, String>) -> String;
}

pub struct Lolalytics;

impl StatisticsUrlProducer for Lolalytics {
    fn get_url(champion: &str, game_mode: &str, params: &HashMap<String, String>) -> String {
        let champion = match champion {
            "Dr. Mundo" => "drmundo",
            "Renata Glasc" => "renata",
            "Nunu & Willump" => "nunu",
            _ => champion,
        }
        .replace(" ", "")
        .replace("'", "")
        .to_lowercase();

        let patch = params.get("patch").map(String::as_str).unwrap_or("30");

        match game_mode {
            "CLASSIC" => format!("https://lolalytics.com/lol/{champion}/build/?patch={patch}"),
            _ => {
                let game_mode = game_mode.replace(" ", "").replace("'", "");
                format!("https://lolalytics.com/lol/{champion}/{game_mode}/build/?patch={patch}")
            }
        }
    }
}
