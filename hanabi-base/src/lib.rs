use rand::{seq::SliceRandom, thread_rng, Rng};
use serde::{Deserialize, Serialize};

const MAX_HINTS: usize = 8;
const MAX_LIVES: usize = 3;

pub type Value = usize;
const MAX_VALUE: Value = 5;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Blue = 0,
    Green = 1,
    Red = 2,
    White = 3,
    Yellow = 4,
    Multi = 5,
}
const MAX_COLORS: usize = 6;
const COLOURS: [Color; 6] = [
    Color::Blue,
    Color::Green,
    Color::Red,
    Color::White,
    Color::Yellow,
    Color::Multi,
];

// Not Copy and Clone to prevent duplicating cards.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[must_use = "Cards cannot disappear"]
pub struct Card {
    pub c: Color,
    pub v: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Deck {
    cards: Vec<Card>,
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
        Self { cards }
    }
    fn take(&mut self) -> Card {
        self.cards.pop().unwrap()
    }
    fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Played(Vec<usize>);

impl Played {
    fn new(variant: GameVariant) -> Self {
        Played(vec![0; variant.num_colors()])
    }

    pub fn score(&self) -> usize {
        self.0.iter().sum()
    }

    /// Returns the card
    fn play(&mut self, card: Card) -> Result<Card, Card> {
        let cur_cnt = &mut self.0[card.c as usize];
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
pub struct CardKnowledge {
    /// NOTE: Indices are 1 lower than values.
    pub vs: [KnowledgeState; MAX_VALUE],
    pub cs: [KnowledgeState; MAX_COLORS],
}

impl CardKnowledge {
    fn new(variant: GameVariant) -> Self {
        let mut this = Self {
            vs: [KnowledgeState::Possible; MAX_VALUE],
            cs: [KnowledgeState::Possible; MAX_COLORS],
        };
        // Disable Multi possibility if needed.
        if variant.num_colors() == 5 {
            this.cs[5] = KnowledgeState::Impossible;
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
            .map(|_| (deck.take(), CardKnowledge::new(variant)))
            .collect();
        Self::Visible(cards)
    }
    fn draw(&mut self, variant: GameVariant, deck: &mut Deck) {
        let Hand::Visible(cards) = self else { panic!() };
        cards.push((deck.take(), CardKnowledge::new(variant)))
    }
    fn take(&mut self, index: usize) -> Option<Card> {
        let Hand::Visible(cards) = self else { panic!() };
        if index < cards.len() {
            Some(cards.remove(index).0)
        } else {
            None
        }
    }
    fn hint(&mut self, hint: Hint) -> Result<(), &'static str> {
        use KnowledgeState::*;
        let Hand::Visible(cards) = self else { panic!() };
        match hint {
            ValueHint(v) => {
                if !(1..MAX_VALUE).contains(&v) {
                    return Err("Hinted value is out of range.");
                }
                for (card, know) in cards {
                    if v == card.v {
                        // Answer to hint is 'yes': fix the value of the card.
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
                for (card, know) in cards {
                    if card.c == c || card.c == Color::Multi {
                        // Answer to hint is 'yes': remove other non-multi colors.
                        for ci in COLOURS {
                            if ci != c && ci != Color::Multi {
                                know.cs[ci as usize] = Impossible;
                            }
                        }
                    } else {
                        // Answer to hint is 'no'.
                        know.cs[Color::Multi as usize] = Impossible;
                        know.cs[c as usize] = Impossible;
                    }

                    // If only one 'possible' remaining, set it to Known.
                    if know.cs.iter().filter(|&&s| s == Possible).count() == 1 {
                        *know.cs.iter_mut().find(|&&mut s| s == Possible).unwrap() = Known;
                    }
                }
            }
        }
        Ok(())
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Play {
    pub index: usize,
    //pub card: Card,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Discard {
    pub index: usize,
    //pub card: Card,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MoveType {
    Play(Play),
    Discard(Discard),
    Hint { player: Player, hint: Hint },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Move {
    player: Player,
    mov: MoveType,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
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
    moves: Vec<Move>,
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
            moves: vec![],
        }
    }

    pub fn make_move(&mut self, mov: Move) -> Result<(), &'static str> {
        let p = self.next_player.ok_or("Game has ended.")?;
        if mov.player != p {
            return Err("Not this player's turn.");
        }

        // Do the move
        match mov.mov {
            MoveType::Play(Play { index }) => {
                let card = self.hands[p]
                    .take(index)
                    .ok_or("Card index out of range.")?;

                // Play the card if possible.
                match self.played.play(card) {
                    Ok(card) => {
                        if card.v == MAX_VALUE {
                            self.hints += 1;
                        }
                        drop(card);
                    }
                    Err(card) => {
                        self.discarded.push(card);
                        self.lives -= 1;
                    }
                }

                self.hands[p].draw(self.variant, &mut self.deck);
            }
            MoveType::Discard(Discard { index }) => {
                if self.hints == MAX_HINTS {
                    return Err("Already at max hints; discarding not allowed.");
                }
                let card = self.hands[p]
                    .take(index)
                    .ok_or("Card index out of range.")?;
                self.discarded.push(card);
                self.hints += 1;
            }
            MoveType::Hint { player, hint } => {
                if self.hints == 0 {
                    return Err("No hints remaining; hinting not allowed.");
                }
                if player == p {
                    return Err("Hinting yourself is not allowed.");
                }
                if !(0..self.num_players).contains(&player) {
                    return Err("Player out of range");
                }
                self.hints -= 1;
                self.hands[player].hint(hint)?;
            }
        }

        // This player will have the last turn?
        if self.deck.is_empty() && self.last_player.is_none() {
            self.last_player = Some(p);
        }

        // End the game?
        self.next_player = if self.lives == 0 || self.last_player == Some(p) {
            None
        } else {
            Some((p + 1) % self.num_players)
        };
        Ok(())
    }

    /// Create a view for the given player, with secret information removed.
    pub fn to_view(&self, player: Player) -> Self {
        let mut view = self.clone();
        view.deck = Deck::default();
        view.hands[player].to_view();
        view
    }
}
