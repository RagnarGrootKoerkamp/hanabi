use turnbased_game_server::server::Server;

#[tokio::main]
async fn main() {
    let address = "127.0.0.1:9284";
    Server::<hanabi_base::Game>::start(address).await;
}
