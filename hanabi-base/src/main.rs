use hanabi_base::{Game, GameVariant};
use owo_colors::OwoColorize;
use text_io::{read, try_read};

pub fn main() {
    eprintln!("Number of players? [3]");
    eprint!(" ");
    let num_players: usize = try_read!("{}\n").unwrap_or(3);
    eprintln!("Variant? [Base] Base | Multi | MultiHard ");
    eprint!(" ");
    let variant: GameVariant = try_read!("{}\n").unwrap_or(GameVariant::Base);
    let players = (1..)
        .take(num_players)
        .map(|id| format!("Player{id}"))
        .collect();
    let mut game = Game::new(players, variant);
    while let Some(next_player) = game.next_player() {
        eprintln!("{}", game.to_view(next_player));
        eprintln!("{}", "move:".bold());
        loop {
            eprint!(" ");
            let mov: String = read!("{}\n");
            let action = match mov.parse() {
                Ok(m) => m,
                Err(err) => {
                    eprintln!("{}", err);
                    eprintln!("move: play <index> | discard <index> | hint <player> <color|value>");
                    continue;
                }
            };
            if let Err(err) = game.act(next_player, action) {
                eprintln!("{}", err);
                eprintln!("move: play <index> | discard <index> | hint <player> <color|value>");
                continue;
            }
            break;
        }
    }

    eprintln!("The game is over");
    eprintln!("{game}");
}
