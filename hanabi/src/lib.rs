use std::{
    fmt::{Debug, Display},
    ops::{Index, IndexMut},
    str::FromStr,
};

use owo_colors::{OwoColorize, Style};
use rand::{seq::SliceRandom, thread_rng, Rng};
use serde::{Deserialize, Serialize};

const MAX_HINTS: usize = 8;
const MAX_LIVES: usize = 3;

pub type Value = usize;
const MAX_VALUE: Value = 5;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, strum_macros::Display)]
#[strum(ascii_case_insensitive)]
pub enum Color {
    Blue = 0,
    Green = 1,
    Red = 2,
    White = 3,
    Yellow = 4,
    Multi = 5,
}
const MAX_COLORS: usize = 6;
const COLORS: [Color; 6] = [
    Color::Blue,
    Color::Green,
    Color::Red,
    Color::White,
    Color::Yellow,
    Color::Multi,
];
const COLORWIDTH: usize = 6 + 1;

impl FromStr for Color {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "" => Err("Color must not be empty"),
            s if "blue".starts_with(s) => Ok(Color::Blue),
            s if "green".starts_with(s) => Ok(Color::Green),
            s if "red".starts_with(s) => Ok(Color::Red),
            s if "white".starts_with(s) => Ok(Color::White),
            s if "yellow".starts_with(s) => Ok(Color::Yellow),
            s if "multi".starts_with(s) => Ok(Color::Multi),
            _ => Err("Unknown color"),
        }
    }
}

impl Color {
    fn to_style(&self) -> Style {
        match self {
            Color::Blue => Style::new().bright_blue(),
            Color::Green => Style::new().green(),
            Color::Red => Style::new().red(),
            Color::White => Style::new().white(),
            Color::Yellow => Style::new().yellow(),
            Color::Multi => Style::new().purple(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ColorArray<T>([T; MAX_COLORS]);
impl<T> ColorArray<T> {
    pub fn find_eq(&self, t: T) -> Option<Color>
    where
        T: Eq + Copy,
    {
        for c in COLORS {
            if self[c] == t {
                return Some(c);
            }
        }
        None
    }
    pub fn count_eq(&self, t: T) -> usize
    where
        T: Eq + Copy,
    {
        self.0.iter().filter(|&&x| x == t).count()
    }
}
impl<T> Index<Color> for ColorArray<T> {
    type Output = T;
    fn index(&self, c: Color) -> &Self::Output {
        &self.0[c as usize]
    }
}
impl<T> IndexMut<Color> for ColorArray<T> {
    fn index_mut(&mut self, c: Color) -> &mut Self::Output {
        &mut self.0[c as usize]
    }
}

// Not Copy and Clone to prevent duplicating cards.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[must_use = "Cards cannot disappear"]
pub struct Card {
    pub c: Color,
    pub v: Value,
}
const CARDWIDTH: usize = COLORWIDTH + 2;

impl Display for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.c.to_style().fmt_prefix(f)?;
        f.pad(&format!("{} {}", self.c, self.v))?;
        self.c.to_style().fmt_suffix(f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum Deck {
    Visible(Vec<Card>),
    Hidden(usize),
}

impl Deck {
    fn count(variant: GameVariant, c: Color, v: Value) -> usize {
        if c == Color::Multi && variant == GameVariant::MultiHard {
            return 1;
        }
        match v {
            1 => 3,
            2 | 3 | 4 => 2,
            5 => 1,
            _ => panic!(),
        }
    }
    fn new(variant: GameVariant) -> Self {
        let mut cards = vec![];
        for c in variant.colors() {
            for v in 1..=MAX_VALUE {
                for _ in 0..Deck::count(variant, c, v) {
                    cards.push(Card { c, v });
                }
            }
        }
        cards.shuffle(&mut thread_rng());
        Deck::Visible(cards)
    }
    fn take(&mut self) -> Option<Card> {
        let Deck::Visible(cards) = self else { panic!() };
        cards.pop()
    }
    fn is_empty(&self) -> bool {
        match self {
            Deck::Visible(cards) => cards.is_empty(),
            Deck::Hidden(len) => *len == 0,
        }
    }
    fn len(&self) -> usize {
        match self {
            Deck::Visible(cards) => cards.len(),
            Deck::Hidden(len) => *len,
        }
    }
    fn view(&mut self) {
        let cards = std::mem::replace(self, Deck::Hidden(0));
        *self = Deck::Hidden(cards.len());
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Played(Vec<usize>);

impl Index<Color> for Played {
    type Output = usize;

    fn index(&self, c: Color) -> &Self::Output {
        &self.0[c as usize]
    }
}

impl IndexMut<Color> for Played {
    fn index_mut(&mut self, c: Color) -> &mut Self::Output {
        &mut self.0[c as usize]
    }
}

impl Played {
    fn new(variant: GameVariant) -> Self {
        Played(vec![0; variant.num_colors()])
    }

    pub fn score(&self) -> usize {
        self.0.iter().sum()
    }

    /// Returns the card
    fn play(&mut self, card: Card) -> Result<Card, Card> {
        let cur_cnt = &mut self[card.c];
        if card.v != *cur_cnt + 1 {
            Err(card)
        } else {
            *cur_cnt += 1;
            Ok(card)
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum KnowledgeState {
    #[default]
    Possible,
    Known,
    Impossible,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Turn {
    Start,
    Turn(usize),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CardKnowledge {
    /// NOTE: Indices are 1 lower than values.
    pub vs: [KnowledgeState; MAX_VALUE],
    pub cs: ColorArray<KnowledgeState>,
    pub picked_up: Turn,
}

impl Display for CardKnowledge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KnowledgeState::*;
        // c:
        // known:
        // red/.../?
        //
        // multi + one other:
        // red/... + italics or *
        //
        // else:
        // ?

        // TODO: Handling of multi.
        let c = self.cs.find_eq(Known);

        // v: 1/2/3/4/5 or ?
        let v = self.vs.iter().position(|&k| k == Known);
        let v = match v {
            Some(v) => b'1' + v as u8,
            None => b'?',
        } as char;

        let (text, style) = match (c, v) {
            (None, '?') => ("?".into(), None),
            (None, _) => (v.to_string(), Some(Style::new().bold())),
            (Some(c), '?') => (c.to_string(), Some(c.to_style().bold())),
            (Some(c), _) => (format!("{c} {v}"), Some(c.to_style().bold())),
        };

        if let Some(style) = style {
            style.fmt_prefix(f)?;
        }
        f.pad(&text)?;
        if let Some(style) = style {
            style.fmt_suffix(f)?;
        }
        Ok(())
    }
}

impl CardKnowledge {
    fn new(variant: GameVariant, turn: Turn) -> Self {
        use KnowledgeState::*;
        let mut this = Self {
            vs: [Possible; MAX_VALUE],
            cs: ColorArray([Possible; MAX_COLORS]),
            picked_up: turn,
        };
        // Disable Multi possibility if needed.
        if !variant.has_multi() {
            this.cs[Color::Multi] = Impossible;
        }
        this
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CardWithKnowledge(Card, CardKnowledge);

impl Display for CardWithKnowledge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KnowledgeState::*;
        // Put an underline under the color/value once it is known.

        let to_style = |k| {
            if k {
                Style::new().underline()
            } else {
                Style::new()
            }
        };
        let color_style = to_style(self.1.cs.find_eq(Known).is_some());
        let value_style = to_style(self.1.vs.iter().position(|&x| x == Known).is_some());

        let len = format!("{} {}", self.0.c, self.0.v).len();
        if let Some(width) = f.width() {
            write!(
                f,
                "{}{} {}{}",
                " ".repeat((width - len as usize) / 2),
                self.0.c.style(color_style).style(self.0.c.to_style()),
                self.0.v.style(value_style).style(self.0.c.to_style()),
                " ".repeat((width - len as usize + 1) / 2),
            )?;
        } else {
            write!(
                f,
                "{} {}",
                self.0.c.style(color_style).style(self.0.c.to_style()),
                self.0.v.style(value_style).style(self.0.c.to_style()),
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Hand {
    Visible(Vec<CardWithKnowledge>),
    Hidden(Vec<CardKnowledge>),
}

impl Hand {
    fn new(variant: GameVariant, cards_per_player: usize, deck: &mut Deck) -> Self {
        let cards = (0..cards_per_player)
            .map(|_| {
                CardWithKnowledge(
                    deck.take().unwrap(),
                    CardKnowledge::new(variant, Turn::Start),
                )
            })
            .collect();
        Self::Visible(cards)
    }
    fn draw(&mut self, variant: GameVariant, deck: &mut Deck) {
        let Hand::Visible(cards) = self else { panic!() };
        if let Some(card) = deck.take() {
            cards.push(CardWithKnowledge(
                card,
                CardKnowledge::new(variant, Turn::Start),
            ));
        }
    }
    fn take(&mut self, card_idx: CardIdx) -> Option<CardWithKnowledge> {
        let Hand::Visible(cards) = self else { panic!() };
        if card_idx.0 - 1 < cards.len() {
            Some(cards.remove(card_idx.0 - 1))
        } else {
            None
        }
    }
    /// Returns the hinted indices.
    fn hint(&mut self, hint: Hint) -> Result<Vec<CardIdx>, &'static str> {
        use KnowledgeState::*;
        let Hand::Visible(cards) = self else { panic!() };
        let mut card_indices = vec![];
        match hint {
            ValueHint(v) => {
                if !(1..=MAX_VALUE).contains(&v) {
                    return Err("Hinted value is out of range.");
                }
                for (idx, CardWithKnowledge(card, know)) in cards.iter_mut().enumerate() {
                    if v == card.v {
                        // Answer to hint is 'yes': fix the value of the card.
                        card_indices.push(CardIdx(idx + 1));
                        know.vs.fill(Impossible);
                        know.vs[v - 1] = Known;
                    } else {
                        // Answer to hint is 'no': remove the possible value.
                        know.vs[v - 1] = Impossible;
                        // If only one 'possible' remaining, set it to Known.
                        if know.vs.iter().filter(|&&s| s == Possible).count() == 1 {
                            *know.vs.iter_mut().find(|&&mut s| s == Possible).unwrap() = Known;
                        }
                    }
                }
            }
            ColorHint(c) => {
                if c == Color::Multi {
                    return Err("Hinting multi is not allowed.");
                }
                for (idx, CardWithKnowledge(card, know)) in cards.iter_mut().enumerate() {
                    if card.c == c || card.c == Color::Multi {
                        // Answer to hint is 'yes': remove other non-multi colors.
                        card_indices.push(CardIdx(idx + 1));
                        for ci in COLORS {
                            if ci != c && ci != Color::Multi {
                                know.cs[ci] = Impossible;
                            }
                        }
                    } else {
                        // Answer to hint is 'no'.
                        know.cs[Color::Multi] = Impossible;
                        know.cs[c] = Impossible;
                    }

                    // If only one 'possible' remaining, set it to Known.
                    if know.cs.0.iter().filter(|&&s| s == Possible).count() == 1 {
                        *know.cs.0.iter_mut().find(|&&mut s| s == Possible).unwrap() = Known;
                    }
                }
            }
        }
        Ok(card_indices)
    }
    fn to_view(&mut self) {
        let Hand::Visible(cards) = std::mem::replace(self, Hand::Hidden(vec![])) else { panic!() };
        *self = Hand::Hidden(
            cards
                .into_iter()
                .map(|CardWithKnowledge(_card, know)| know)
                .collect(),
        );
    }
}

/// 0-based player index. Shown to user as 1-based.
pub type Player = usize;
/// 1-based card index.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct CardIdx(usize);

impl FromStr for CardIdx {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let idx = s.parse().map_err(|_| "Failed to parse card index.")?;
        if idx == 0 {
            Err("Index must not be 0.")
        } else {
            Ok(CardIdx(idx))
        }
    }
}

impl Display for CardIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Hint {
    ValueHint(Value),
    ColorHint(Color),
}
pub use Hint::*;

impl FromStr for Hint {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(c) = s.parse() {
            Ok(ColorHint(c))
        } else if let Ok(v) = s.parse() {
            Ok(ValueHint(v))
        } else {
            Err("Could not parse hint as color or value")
        }
    }
}

impl Display for Hint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueHint(v) => write!(f, "{}", v.bold()),
            ColorHint(c) => write!(f, "{}", c.style(c.to_style()).bold()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Move {
    Play { card_idx: CardIdx },
    Discard { card_idx: CardIdx },
    Hint { hinted_player: Player, hint: Hint },
    HintOtherPlayer { hint: Hint },
}

impl FromStr for Move {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        let mov = match tokens.next().ok_or("Empty string")? {
            "" => return Err("Empty action"),
            a if "play".starts_with(a) => Move::Play {
                card_idx: tokens.next().ok_or("Missing index")?.parse()?,
            },
            a if "discard".starts_with(a) => Move::Discard {
                card_idx: tokens.next().ok_or("Missing index")?.parse()?,
            },
            a if "hint".starts_with(a) => {
                if tokens.clone().count() == 2 {
                    Move::Hint {
                        hinted_player: tokens
                            .next()
                            .ok_or("Missing player")?
                            .parse::<usize>()
                            .map_err(|_| "Could not parse player.")?
                            - 1,
                        hint: tokens.next().ok_or("Missing hint")?.parse()?,
                    }
                } else {
                    Move::HintOtherPlayer {
                        hint: tokens.next().ok_or("Missing hint")?.parse()?,
                    }
                }
            }

            _ => return Err("Unknown action"),
        };
        if tokens.next().is_some() {
            return Err("Trailing tokens");
        }
        Ok(mov)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MoveLog {
    Play {
        card_idx: CardIdx,
        card: Card,
        know: CardKnowledge,
        success: bool,
    },
    Discard {
        card_idx: CardIdx,
        card: Card,
        know: CardKnowledge,
    },
    Hint {
        hinted_player: Player,
        hint: Hint,
        card_indices: Vec<CardIdx>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerMoveLog {
    pub player: Player,
    pub mov: MoveLog,
}

pub struct PlayerMoveLogWithNames<'a> {
    pub mov: &'a PlayerMoveLog,
    pub players: &'a Vec<String>,
}

impl<'a> Display for PlayerMoveLogWithNames<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            mov: PlayerMoveLog { player, mov },
            players: names,
        } = self;
        let player = &names[*player];
        match mov {
            MoveLog::Play {
                card_idx,
                card,
                know,
                success,
            } => {
                if *success {
                    write!(
                        f,
                        "{} played the {} from position {card_idx} knowing {know}.",
                        player.bold(),
                        card.bold(),
                    )
                } else {
                    write!(
                        f,
                        "{} {} the {} from position {card_idx} knowing {know}, and {}.",
                        player.bold(),
                        "played".red(),
                        card.bold(),
                        "LOST A LIFE".red()
                    )
                }
            }
            MoveLog::Discard {
                card_idx,
                card,
                know,
            } => write!(
                f,
                "{} discarded the {} from position {card_idx} knowing {know}.",
                player.bold(),
                card.bold(),
            ),
            MoveLog::Hint {
                hinted_player,
                hint,
                card_indices,
            } => {
                let hinted_player = &names[*hinted_player];
                write!(
                    f,
                    "{} hinted {} {hint} at pos [",
                    player.bold(),
                    hinted_player.bold(),
                )?;
                for (idx, card_idx) in card_indices.iter().enumerate() {
                    if idx == 0 {
                        write!(f, "{card_idx}")?;
                    } else {
                        write!(f, ",{card_idx}")?;
                    }
                }
                write!(f, "].")
            }
        }
    }
}

#[derive(
    Debug,
    Serialize,
    Deserialize,
    Clone,
    Copy,
    PartialEq,
    Eq,
    strum_macros::Display,
    strum_macros::EnumString,
)]
#[strum(ascii_case_insensitive)]
pub enum GameVariant {
    Base,
    Multi,
    MultiHard,
}

impl GameVariant {
    pub fn num_colors(&self) -> usize {
        match self {
            GameVariant::Base => 5,
            GameVariant::Multi | GameVariant::MultiHard => 6,
        }
    }
    pub fn max_score(&self) -> usize {
        5 * self.num_colors()
    }
    pub fn has_multi(&self) -> bool {
        match self {
            GameVariant::Base => false,
            GameVariant::Multi | GameVariant::MultiHard => true,
        }
    }
    pub fn colors(&self) -> Vec<Color> {
        use Color::*;
        match self {
            GameVariant::Base => vec![Blue, Green, Red, White, Yellow],
            GameVariant::Multi | GameVariant::MultiHard => {
                vec![Blue, Green, Red, White, Yellow, Multi]
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy)]
pub enum GameState {
    NextPlayer(Player),
    Won,
    Died,
    Ended,
}

impl GameState {
    fn has_ended(&self) -> bool {
        match self {
            GameState::NextPlayer(_) => false,
            _ => true,
        }
    }

    fn to_string(&self, players: &Vec<String>) -> String {
        match *self {
            GameState::NextPlayer(player) => format!("next: {}", players[player]),
            GameState::Won => "won".green().to_string(),
            GameState::Died => "died".red().to_string(),
            GameState::Ended => "ended".red().to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Game {
    // data
    players: Vec<String>,
    start_player: Player,
    /// None when the game has ended.
    game_state: GameState,
    /// Set as soon as the deck is empty.
    last_player: Option<Player>,

    cards_per_player: usize,
    hints: usize,
    lives: usize,
    variant: GameVariant,

    // cards
    deck: Deck,
    hands: Vec<Hand>,
    discarded: Vec<Card>,
    played: Played,

    // move
    move_log: Vec<PlayerMoveLog>,
}

impl Game {
    pub fn new(mut players: Vec<String>, variant: GameVariant) -> Self {
        let num_players = players.len();
        players.shuffle(&mut rand::thread_rng());
        let start_player = thread_rng().gen_range(0..num_players);
        let cards_per_player = match num_players {
            2 | 3 => 5,
            4 | 5 => 4,
            _ => panic!(),
        };
        let mut deck = Deck::new(variant);
        let hands = (0..num_players)
            .map(|_| Hand::new(variant, cards_per_player, &mut deck))
            .collect();

        Self {
            players,
            start_player,
            game_state: GameState::NextPlayer(start_player),
            last_player: None,
            cards_per_player,
            hints: MAX_HINTS,
            lives: MAX_LIVES,
            variant,
            deck,
            hands,
            discarded: vec![],
            played: Played::new(variant),
            move_log: vec![],
        }
    }

    pub fn player_id(&self, player: &String) -> Option<Player> {
        self.players.iter().position(|x| x == player)
    }

    pub fn make_move(&mut self, player: Player, mov: Move) -> Result<(), &'static str> {
        let GameState::NextPlayer(next_player) = self.game_state else {
            return Err("Game has ended.")?;
        };
        if player != next_player {
            return Err("Not this player's turn.");
        }

        // Do the move.
        match mov {
            Move::Play { card_idx } => {
                let CardWithKnowledge(card, know) = self.hands[player]
                    .take(card_idx)
                    .ok_or("Card index out of range.")?;

                // Play the card if possible.
                // Card is cloned for the log.
                let success = match self.played.play(card.clone()) {
                    Ok(card) => {
                        if card.v == MAX_VALUE {
                            self.hints += 1;
                        }
                        drop(card);
                        true
                    }
                    Err(card) => {
                        self.discarded.push(card);
                        self.lives -= 1;
                        false
                    }
                };

                self.hands[player].draw(self.variant, &mut self.deck);
                self.move_log.push(PlayerMoveLog {
                    player,
                    mov: MoveLog::Play {
                        card_idx,
                        card,
                        know,
                        success,
                    },
                })
            }
            Move::Discard { card_idx } => {
                if self.hints == MAX_HINTS {
                    return Err("Already at max hints; discarding not allowed.");
                }
                let CardWithKnowledge(card, know) = self.hands[player]
                    .take(card_idx)
                    .ok_or("Card index out of range.")?;
                self.discarded.push(card.clone());
                self.hints += 1;
                self.hands[player].draw(self.variant, &mut self.deck);
                self.move_log.push(PlayerMoveLog {
                    player,
                    mov: MoveLog::Discard {
                        card_idx,
                        card,
                        know,
                    },
                })
            }
            Move::Hint {
                hinted_player,
                hint,
            } => {
                self.hint(hinted_player, player, hint)?;
            }
            Move::HintOtherPlayer { hint } => {
                if self.players.len() == 2 {
                    self.hint((player + 1) % 2, player, hint)?;
                } else {
                    return Err("Specify the player to hint");
                }
            }
        }

        // End the game?
        self.game_state = if self.lives == 0 {
            GameState::Died
        } else if self.played.score() == self.variant.max_score() {
            GameState::Won
        } else if self.last_player == Some(player) {
            GameState::Ended
        } else {
            GameState::NextPlayer((player + 1) % self.players.len())
        };

        // This player will have the last turn?
        if self.deck.is_empty() && self.last_player.is_none() {
            self.last_player = Some(player);
        }

        Ok(())
    }

    fn hint(
        &mut self,
        hinted_player: usize,
        player: usize,
        hint: Hint,
    ) -> Result<(), &'static str> {
        if self.hints == 0 {
            return Err("No hints remaining; hinting not allowed.");
        }
        if hinted_player == player {
            return Err("Hinting yourself is not allowed.");
        }
        if !(0..self.players.len()).contains(&hinted_player) {
            return Err("Player out of range");
        }
        self.hints -= 1;
        let card_indices = self.hands[hinted_player].hint(hint.clone())?;
        Ok(self.move_log.push(PlayerMoveLog {
            player,
            mov: MoveLog::Hint {
                hinted_player,
                hint,
                card_indices,
            },
        }))
    }

    /// Create a view for the given player, with secret information removed.
    pub fn to_view(&self, player: Player) -> Self {
        let mut view = self.clone();
        view.deck.view();
        view.hands[player].to_view();
        view
    }

    pub fn game_state(&self) -> GameState {
        self.game_state
    }

    pub fn has_ended(&self) -> bool {
        self.game_state.has_ended()
    }
}

/// Print the current game state to stderr.
///
/// log:
///  <log>
///
/// turn: 0 | hints: 8 | lives: 3
///
///   played | discarded:
/// red    0 | 0 0 0 0 0
/// green  0 | 0 0 0 0 0
/// blue   0 | 0 0 0 0 0
/// yellow 0 | 0 0 0 0 0
/// green  0 | 0 0 0 0 0
/// multi  0 | 0 0 0 0 0
///
///  p 1        2        3        4        5
/// *1 green 1
///  2 yellow 5
///  3 5        yellow
impl Display for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n------------------------------------------\n")?;

        let good = Style::new().green();
        let ok = Style::new().white();
        let warn = Style::new().yellow();
        let error = Style::new().red();

        let hints_style = match self.hints {
            0 => error,
            1 | 2 => warn,
            _ => good,
        };

        let lives_style = match self.lives {
            MAX_LIVES => good,
            0 => error,
            _ => warn,
        };

        let deck_style = match self.deck.len() {
            0 => error,
            1..=5 => warn,
            _ => good,
        };

        write!(
            f,
            "Hints: {} | Lives: {} | Deck: {} | Score: {} | Turn: {}",
            self.hints.style(hints_style).bold(),
            self.lives.style(lives_style).bold(),
            self.deck.len().style(deck_style).bold(),
            self.played.score().bold(),
            self.move_log.len().bold(),
        )?;
        if let Some(last_player) = self.last_player {
            writeln!(
                f,
                " | {}",
                format!("Last move: {}", self.players[last_player])
                    .red()
                    .bold()
            )?;
        } else {
            writeln!(f)?;
        }

        writeln!(f)?;
        writeln!(f, "    {} | {}", "played".bold(), "discarded".bold())?;
        let mut discarded = [[0; MAX_VALUE]; MAX_COLORS];
        for card in &self.discarded {
            discarded[card.c as usize][card.v - 1] += 1;
        }
        for c in self.variant.colors() {
            write!(f, " {:COLORWIDTH$}", c.style(c.to_style()))?;
            write!(
                f,
                " {} {}",
                self.played[c].bold().style(c.to_style()),
                "|".style(c.to_style())
            )?;
            for v in 1..=MAX_VALUE {
                let d = discarded[c as usize][v - 1];
                let style = if v <= self.played[c] {
                    good.bold()
                } else if d == 0 {
                    ok
                } else if d == Deck::count(self.variant, c, v) {
                    error
                } else {
                    warn
                };
                write!(f, " {}", d.style(style))?;
            }
            writeln!(f)?;
        }
        writeln!(f)?;

        write!(f, " {:13} ", "")?;
        for idx in 1..=self.cards_per_player {
            write!(f, " {idx:^CARDWIDTH$}")?;
        }
        writeln!(f)?;
        for (pid, p) in self.players.iter().enumerate() {
            let this_turn_style = if self.game_state == GameState::NextPlayer(pid) {
                Style::new().bold()
            } else {
                Style::new()
            };
            write!(
                f,
                "{}",
                format!(" {}: {p:10} ", pid + 1).style(this_turn_style)
            )?;
            match &self.hands[pid] {
                Hand::Visible(hand) => {
                    for card_with_know in hand {
                        write!(f, " {card_with_know:^CARDWIDTH$}")?;
                    }
                }
                Hand::Hidden(hand) => {
                    for know in hand {
                        write!(f, " {know:^CARDWIDTH$}")?;
                    }
                }
            };
            writeln!(f)?;
        }
        writeln!(f)?;
        writeln!(f, "{}", "log:".bold())?;
        for (id, mov) in self
            .move_log
            .iter()
            .enumerate()
            .rev()
            .take(self.players.len())
            .rev()
        {
            writeln!(
                f,
                " {:2}: {}",
                id + 1,
                PlayerMoveLogWithNames {
                    mov,
                    players: &self.players
                }
            )?;
        }
        writeln!(f, "{}", self.game_state.to_string(&self.players).bold())?;
        Ok(())
    }
}

impl turnbased_game_server::GameT for Game {
    type Settings = GameVariant;
    type Move = Move;

    fn new(players: Vec<String>, variant: Self::Settings) -> Self {
        Self::new(players, variant)
    }

    fn make_move(&mut self, player: &String, mov: Move) -> Result<(), &'static str> {
        Self::make_move(self, self.player_id(player).ok_or("Player not found")?, mov)
    }

    fn to_view(&self, player: &String) -> Self {
        match self.player_id(player) {
            Some(player) => self.to_view(player),
            None => self.clone(),
        }
    }

    fn move_help() -> &'static str {
        "play <index> | discard <index> | hint <playerid> <color|value>"
    }
}
