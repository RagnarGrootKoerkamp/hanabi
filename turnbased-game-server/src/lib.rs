pub mod client;
pub mod server;
pub mod types;

use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

pub use client::start_client;
pub use server::start_server;

/// Trait that supported games must implement.
pub trait GameT:
    Sized + Debug + Display + Serialize + DeserializeOwned + Clone + Send + 'static
{
    type Settings: Debug + Display + Serialize + DeserializeOwned + Clone + FromStr + Send;
    type Move: Debug + Serialize + DeserializeOwned + Clone + FromStr<Err = &'static str>;
    fn new(player_names: Vec<String>, settings: Self::Settings) -> Self;
    fn make_move(&mut self, player: &String, mov: Self::Move) -> Result<(), &'static str>;
    fn to_view(&self, player: &String) -> Self;
}
