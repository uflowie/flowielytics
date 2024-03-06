use tokio;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        run_lcu_listener(rx).await;
    });

    // listen for incoming websocket connections
    // once a websocket is connected, send a new producer to the lcu listener
    // listen for messages from the listener and send the appropriate html to the websocket
    // can also do server sent events if it's easier, since communication is unidirectional

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        // Do some work or check for a condition to break the loop
    }
}

async fn run_lcu_listener(mut receiver_channel: mpsc::Receiver<mpsc::Sender<LCUState>>) {
    let mut connections: Vec<mpsc::Sender<LCUState>> = Vec::new();
    let mut state = LCUState::NotConnected;

    // loop here and select either:
    // 1. a new connection - add it to the list of connections and send the current state
    // 2. a new state - send the new state to all connections. new state is received from the lcu
    // if we are currently connected to it we listen to the websocket, otherwise we try connecting
    // in a loop and fetch the initial state once we are connected
    loop {
        let sub = receiver_channel
            .recv()
            .await
            .expect("Failed to receive from channel");

        println!("Received new connection");

        state = LCUState::Connected;
        sub.send(state.clone()).await.expect("Failed to send initial state");
        state = LCUState::NotConnected;

        connections.push(sub);

        for conn in connections.iter() {
            conn.send(state.clone()).await.expect("Failed to send state");
        }
    }
}

#[derive(Clone)]
#[derive(Debug)]
enum LCUState {
    NotConnected,
    Connected,
    Playing { champion: String, game_mode: String },
}
