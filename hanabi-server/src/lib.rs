use clap::Parser;

#[derive(Parser)]
pub struct Args {
    address: Option<String>,

    #[arg(long, short)]
    local: bool,
}

impl Args {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
    pub fn server_address(&self) -> &str {
        if self.local {
            "127.0.0.1:38271".into()
        } else {
            if let Some(address) = self.address.as_ref() {
                &address
            } else {
                "127.0.0.1:38271"
            }
        }
    }
    pub fn client_address(&self) -> &str {
        if self.local {
            "ws://127.0.0.1:38271".into()
        } else {
            if let Some(address) = self.address.as_ref() {
                &address
            } else {
                "ws://hanabi.ragnargrootkoerkamp.nl/websocket/"
            }
        }
    }
}
