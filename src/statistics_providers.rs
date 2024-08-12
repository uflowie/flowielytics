pub struct Lolalytics;

pub trait StatisticsUrlProducer {
    fn get_url(champion: &str, game_mode: &str) -> String;
}

impl StatisticsUrlProducer for Lolalytics {
    fn get_url(champion: &str, game_mode: &str) -> String {
        let champion = match champion {
            "Dr. Mundo" => "drmundo",
            "Renata Glasc" => "renata",
            "Nunu & Willump" => "nunu",
            _ => champion,
        }
        .replace(" ", "")
        .to_lowercase();

        let game_mode = game_mode.replace(" ", "").to_lowercase();

        format!("https://lolalytics.com/lol/{champion}/{game_mode}/build/?patch=30")
    }
}
