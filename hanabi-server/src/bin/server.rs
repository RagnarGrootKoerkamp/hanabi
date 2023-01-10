#[tokio::main]
async fn main() {
    let address = "127.0.0.1:9284";
    turnbased_game_server::start_server::<hanabi::Game>(address).await;
}
