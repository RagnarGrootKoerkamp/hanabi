use std::{
    fmt::Display,
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

impl std::fmt::Display for Card {
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
    fn take(&mut self) -> Card {
        let Deck::Visible(cards) = self else { panic!() };
        cards.pop().unwrap()
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

        let c = if let Some(c) = self.cs.find_eq(Known) {
            Some(c)
        } else if self.cs[Color::Multi] == Possible && self.cs.count_eq(Possible) == 2 {
            todo!();
            //format!("{}*", self.cs.find_eq(Possible).unwrap())
        } else {
            None
        };

        // v: 1/2/3/4/5 or ?
        let v = self.vs.iter().position(|&k| k == Known);
        let v = match v {
            Some(v) => b'1' + v as u8,
            None => b'?',
        } as char;

        let (text, style) = match (c, v) {
            (None, '?') => ("?".into(), None),
            (None, _) => (format!("{v}"), None),
            (Some(c), '?') => (c.to_string(), Some(c.to_style())),
            (Some(c), _) => (format!("{c} {v}"), Some(c.to_style())),
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
pub enum Hand {
    Visible(Vec<(Card, CardKnowledge)>),
    Hidden(Vec<CardKnowledge>),
}

impl Hand {
    fn new(variant: GameVariant, cards_per_player: usize, deck: &mut Deck) -> Self {
        let cards = (0..cards_per_player)
            .map(|_| (deck.take(), CardKnowledge::new(variant, Turn::Start)))
            .collect();
        Self::Visible(cards)
    }
    fn draw(&mut self, variant: GameVariant, deck: &mut Deck) {
        let Hand::Visible(cards) = self else { panic!() };
        cards.push((deck.take(), CardKnowledge::new(variant, Turn::Start)))
    }
    fn take(&mut self, index: usize) -> Option<(Card, CardKnowledge)> {
        let Hand::Visible(cards) = self else { panic!() };
        if index < cards.len() {
            Some(cards.remove(index))
        } else {
            None
        }
    }
    /// Returns the hinted positions.
    fn hint(&mut self, hint: Hint) -> Result<Vec<usize>, &'static str> {
        use KnowledgeState::*;
        let Hand::Visible(cards) = self else { panic!() };
        let mut positions = vec![];
        match hint {
            ValueHint(v) => {
                if !(1..=MAX_VALUE).contains(&v) {
                    return Err("Hinted value is out of range.");
                }
                for (pos, (card, know)) in cards.iter_mut().enumerate() {
                    if v == card.v {
                        // Answer to hint is 'yes': fix the value of the card.
                        positions.push(pos);
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
                for (pos, (card, know)) in cards.iter_mut().enumerate() {
                    if card.c == c || card.c == Color::Multi {
                        // Answer to hint is 'yes': remove other non-multi colors.
                        positions.push(pos);
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
        Ok(positions)
    }
    fn to_view(&mut self) {
        let Hand::Visible(cards) = std::mem::replace(self, Hand::Hidden(vec![])) else { panic!() };
        *self = Hand::Hidden(cards.into_iter().map(|(_card, know)| know).collect());
    }
}

pub type Player = usize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Hint {
    ValueHint(Value),
    ColorHint(Color),
}
pub use Hint::*;

impl FromStr for Hint {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(c) = Color::from_str(s) {
            Ok(ColorHint(c))
        } else if let Ok(v) = Value::from_str(s) {
            Ok(ValueHint(v))
        } else {
            Err("Could not parse hint as color or value")
        }
    }
}

impl Display for Hint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueHint(v) => write!(f, "value {v}"),
            ColorHint(c) => write!(f, "color {}", c.style(c.to_style())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Move {
    Play { pos: usize },
    Discard { pos: usize },
    Hint { hinted_player: Player, hint: Hint },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MoveLog {
    Play {
        pos: usize,
        card: Card,
        know: CardKnowledge,
        success: bool,
    },
    Discard {
        pos: usize,
        card: Card,
        know: CardKnowledge,
    },
    Hint {
        hinted_player: Player,
        hint: Hint,
        positions: Vec<usize>,
    },
}

impl FromStr for Move {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        match tokens.next().ok_or("Empty string")? {
            "play" => Ok(Move::Play {
                pos: usize::from_str(tokens.next().ok_or("Missing index")?)
                    .map_err(|_| "Could not parse card index.")?,
            }),
            "discard" => Ok(Move::Discard {
                pos: usize::from_str(tokens.next().ok_or("Missing index")?)
                    .map_err(|_| "Could not parse card index.")?,
            }),
            "hint" => Ok(Move::Hint {
                hinted_player: usize::from_str(tokens.next().ok_or("Missing player")?)
                    .map_err(|_| "Could not parse player.")?,
                hint: Hint::from_str(tokens.next().ok_or("Missing hint")?)?,
            }),

            _ => return Err("Unknown move"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerMove {
    pub player: Player,
    pub mov: Move,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerMoveLog {
    pub player: Player,
    pub mov: MoveLog,
}

impl Display for PlayerMoveLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { player, mov } = self;
        match mov {
            MoveLog::Play {
                pos,
                card,
                know,
                success,
            } => {
                if *success {
                    write!(
                        f,
                        "Player {player} played the {card} from position {pos} knowing {know}."
                    )
                } else {
                    write!(
                        f,
                        "Player {player} played the {card} from position {pos} knowing {know}, and LOST A LIFE."
                    )
                }
            }
            MoveLog::Discard { pos, card, know } => write!(
                f,
                "Player {player} discarded the {card} from position {pos} knowing {know}."
            ),
            MoveLog::Hint {
                hinted_player,
                hint,
                positions,
            } => {
                write!(
                    f,
                    "Player {player} hinted player {hinted_player} {} {} with {hint} at positions [",
                    positions.len(), if positions.len() == 1 { "card" } else {"cards"}
                )?;
                for (idx, pos) in positions.iter().enumerate() {
                    if idx == 0 {
                        write!(f, "{pos}")?;
                    } else {
                        write!(f, ",{pos}")?;
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Game {
    // data
    num_players: Player,
    start_player: Player,
    /// None when the game has ended.
    next_player: Option<Player>,
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
    moves_log: Vec<PlayerMoveLog>,
}

impl Game {
    pub fn new(num_players: Player, variant: GameVariant) -> Self {
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
            num_players,
            start_player,
            next_player: Some(start_player),
            last_player: None,
            cards_per_player,
            hints: MAX_HINTS,
            lives: MAX_LIVES,
            variant,
            deck,
            hands,
            discarded: vec![],
            played: Played::new(variant),
            moves_log: vec![],
        }
    }

    pub fn make_move(&mut self, mov: PlayerMove) -> Result<(), &'static str> {
        let player = self.next_player.ok_or("Game has ended.")?;
        if mov.player != player {
            return Err("Not this player's turn.");
        }

        // Do the move
        match mov.mov {
            Move::Play { pos } => {
                let (card, know) = self.hands[player]
                    .take(pos)
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
                self.moves_log.push(PlayerMoveLog {
                    player,
                    mov: MoveLog::Play {
                        pos,
                        card,
                        know,
                        success,
                    },
                })
            }
            Move::Discard { pos } => {
                if self.hints == MAX_HINTS {
                    return Err("Already at max hints; discarding not allowed.");
                }
                let (card, know) = self.hands[player]
                    .take(pos)
                    .ok_or("Card index out of range.")?;
                self.discarded.push(card.clone());
                self.hints += 1;
                self.hands[player].draw(self.variant, &mut self.deck);
                self.moves_log.push(PlayerMoveLog {
                    player,
                    mov: MoveLog::Discard { pos, card, know },
                })
            }
            Move::Hint {
                hinted_player,
                hint,
            } => {
                if self.hints == 0 {
                    return Err("No hints remaining; hinting not allowed.");
                }
                if hinted_player == player {
                    return Err("Hinting yourself is not allowed.");
                }
                if !(0..self.num_players).contains(&hinted_player) {
                    return Err("Player out of range");
                }
                self.hints -= 1;
                let positions = self.hands[hinted_player].hint(hint.clone())?;
                self.moves_log.push(PlayerMoveLog {
                    player,
                    mov: MoveLog::Hint {
                        hinted_player,
                        hint,
                        positions,
                    },
                })
            }
        }

        // This player will have the last turn?
        if self.deck.is_empty() && self.last_player.is_none() {
            self.last_player = Some(player);
        }

        // End the game?
        self.next_player = if self.lives == 0 || self.last_player == Some(player) {
            None
        } else {
            Some((player + 1) % self.num_players)
        };
        Ok(())
    }

    /// Create a view for the given player, with secret information removed.
    pub fn to_view(&self, player: Player) -> Self {
        let mut view = self.clone();
        view.deck.view();
        view.hands[player].to_view();
        view
    }

    pub fn next_player(&self) -> Option<usize> {
        self.next_player
    }

    pub fn has_ended(&self) -> bool {
        self.next_player.is_none()
    }
}

/// Print the current game state to stderr.
///
/// turn: 0 | hints: 8 | lives: 3
///
/// discarded:
/// red    0 0 0 0 0
/// green  0 0 0 0 0
/// blue   0 0 0 0 0
/// yellow 0 0 0 0 0
/// green  0 0 0 0 0
/// multi  0 0 0 0 0
///
/// played:
/// red 0 | green 1 | blue 2 | green 3 | yellow 4 | multi 5
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

        writeln!(
            f,
            "Hints: {} | Lives: {} | Deck: {} | Score: {} | Turn: {}",
            self.hints.style(hints_style).bold(),
            self.lives.style(lives_style).bold(),
            self.deck.len().style(deck_style).bold(),
            self.played.score().bold(),
            self.moves_log.len().bold(),
        )?;

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
                    good
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

        write!(f, "  ")?;
        for idx in 0..self.cards_per_player {
            write!(f, " {idx:^CARDWIDTH$}")?;
        }
        writeln!(f)?;
        for p in 0..self.num_players {
            let this_turn = if self.next_player == Some(p) {
                '*'
            } else {
                ' '
            };
            write!(f, "{this_turn}{p}")?;
            match &self.hands[p] {
                Hand::Visible(hand) => {
                    for (card, _know) in hand {
                        write!(f, " {card:^CARDWIDTH$}")?;
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
        writeln!(f, "moves:")?;
        for (id, mov) in self
            .moves_log
            .iter()
            .enumerate()
            .rev()
            .take(self.num_players)
            .rev()
        {
            writeln!(f, " {id:2}: {mov}")?;
        }
        Ok(())
    }
}
