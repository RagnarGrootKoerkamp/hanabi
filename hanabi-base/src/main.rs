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
            let err = match mov.parse() {
                Ok(action) => match game.act(next_player, action) {
                    Ok(()) => break,
                    Err(err) => err,
                },
                Err(err) => err,
            };
            eprintln!("{}", err);
            eprintln!(
                "{} play <index> | discard <index> | hint <player> <color|value>",
                "move:".bold()
            );
        }
    }

    eprintln!("The game is over");
    eprintln!("{game}");
}
