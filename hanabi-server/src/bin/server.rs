#[tokio::main]
async fn main() {
    let args = hanabi_server::Args::parse();
    turnbased_game_server::start_server::<hanabi::Game>(args.server_address()).await;
}
