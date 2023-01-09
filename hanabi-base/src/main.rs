use std::str::FromStr;

use hanabi_base::{Action, Game, GameVariant, Move};
use text_io::read;

pub fn main() {
    eprintln!("Number of players? ");
    eprint!(" ");
    let num_players: usize = read!("{}\n");
    eprintln!("Variant? Base | Multi | MultiHard ");
    eprint!(" ");
    let variant: GameVariant = read!("{}\n");
    let mut game = Game::new(num_players, variant);
    while let Some(next_player) = game.next_player() {
        eprintln!("{}", game.to_view(next_player));
        eprintln!("move:");
        loop {
            eprint!(" ");
            let mov: String = read!("{}\n");
            let action = match Action::from_str(&mov) {
                Ok(m) => m,
                Err(err) => {
                    eprintln!("move: play <index> | discard <index> | hint <player> <color|value>");
                    eprintln!("{}", err);
                    continue;
                }
            };
            if let Err(err) = game.act(next_player, action) {
                eprintln!("move: play <index> | discard <index> | hint <player> <color|value>");
                eprintln!("{}", err);
                continue;
            }
            break;
        }
    }

    eprintln!("The game is over");
    eprintln!("{game}");
}
