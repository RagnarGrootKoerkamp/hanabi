#[tokio::main]
async fn main() {
    let args = hanabi_server::Args::parse();
    turnbased_game_server::start_client::<hanabi::Game>(args.client_address()).await;
}
