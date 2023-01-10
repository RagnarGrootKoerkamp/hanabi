#[tokio::main]
async fn main() {
    let address = "ws://127.0.0.1:9284";
    //let address = "ws://crew.ragnargrootkoerkamp.nl/websocket/";
    turnbased_game_server::start_client::<hanabi_base::Game>(address).await;
}
