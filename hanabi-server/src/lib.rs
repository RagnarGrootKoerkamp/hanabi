use clap::Parser;

#[derive(Parser)]
pub struct Args {
    #[arg(default_value = "ws://hanabi.ragnargrootkoerkamp.nl/websocket/")]
    address: String,

    #[arg(long, short)]
    local: bool,
}

impl Args {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
    pub fn server_address(&self) -> &str {
        "127.0.0.1:9284"
    }
    pub fn client_address(&self) -> &str {
        if self.local {
            "ws://127.0.0.1:9284".into()
        } else {
            &self.address
        }
    }
}
