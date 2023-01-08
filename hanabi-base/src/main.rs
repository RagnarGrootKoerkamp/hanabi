use std::str::FromStr;

use hanabi_base::{Game, GameVariant, Move, PlayerMove};
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
            let mov = match Move::from_str(&mov) {
                Ok(m) => m,
                Err(err) => {
                    eprintln!("move: play <index> | discard <index> | hint <player> <color|value>");
                    eprintln!("{}", err);
                    continue;
                }
            };
            if let Err(err) = game.make_move(PlayerMove {
                player: next_player,
                mov,
            }) {
                eprintln!("move: play <index> | discard <index> | hint <player> <color|value>");
                eprintln!("{}", err);
                continue;
            }
            break;
        }
    }
}
