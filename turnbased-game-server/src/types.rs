use crate::GameT;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

// TODO: Separate Player id and name. For now the name is the id.
pub type UserId = String;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct RoomId(pub usize);

impl Display for RoomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RoomId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RoomId(s.parse().map_err(|_| "Could not parse room id")?))
    }
}

pub type ClientId = std::net::SocketAddr;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound = "")]
pub enum RoomState<Game: GameT> {
    WaitingForPlayers {
        min_players: usize,
        max_players: usize,
    },
    // Game is None when viewing the list of all games.
    Started(Option<Game>),
    Ended(Option<Game>),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(bound = "")]
pub struct Room<Game: GameT> {
    pub roomid: RoomId,
    pub settings: Game::Settings,
    pub players: Vec<UserId>,
    pub state: RoomState<Game>,
}

impl<Game: GameT> Display for Room<Game> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use RoomState::*;
        let Room {
            roomid,
            settings,
            players,
            state,
        } = &self;

        let status = match state {
            WaitingForPlayers { .. } => "pending",
            Started(_) => "started",
            Ended(_) => "ended",
        };
        match state {
            RoomState::WaitingForPlayers {
                min_players,
                max_players,
            } => {
                write!(
                    f,
                    "{roomid:>5}: {status:7} {settings:<20} {min_players}-{max_players}  {}",
                    players.join(", ")
                )
            }
            Started(None) | Ended(None) => {
                write!(
                    f,
                    "{roomid:>5}: {status:7} {settings:<20}     {}",
                    players.join(", ")
                )
            }
            Started(Some(g)) | Ended(Some(g)) => {
                write!(f, "{}", g)
            }
        }
    }
}

/// An action that can be sent over an incoming websocket.
#[derive(Serialize, Deserialize)]
pub enum Action<Game: GameT> {
    /// Which user is using the socket.
    Login(UserId),
    /// User stopped used the socket.
    Logout,

    /// View a room and subscribe to updates.
    WatchRoom(RoomId),
    /// Stop viewing a room. Tells the server to stop sending updates for the
    /// viewed room.
    LeaveRoom,

    /// Create a new room.
    NewRoom {
        min_players: usize,
        max_players: usize,
        settings: Game::Settings,
    },
    /// Join the current room if it is waiting for players.
    JoinRoom,

    /// Start the game in the current room.
    StartGame,

    /// Make a move in the current room.
    MakeMove(Game::Move),
}

impl<Game: GameT> FromStr for Action<Game> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Action::*;
        let mut tokens = s.split_ascii_whitespace();
        let mov = match tokens.next().ok_or("Empty string")? {
            "login" => Login(tokens.next().ok_or("missing user id")?.into()),
            "logout" => Logout,
            "enter" => WatchRoom(tokens.next().ok_or("missing room id")?.parse()?),
            "leave" => LeaveRoom,
            "new" => NewRoom {
                min_players: tokens
                    .next()
                    .ok_or("missing min players")?
                    .parse()
                    .map_err(|_| "failed to parse min_players")?,
                max_players: tokens
                    .next()
                    .ok_or("missing max players")?
                    .parse()
                    .map_err(|_| "failed to parse max_players")?,
                settings: {
                    let s = Itertools::intersperse(tokens, " ")
                        .collect::<String>()
                        .parse()
                        .map_err(|_| "Could not parse settings")?;
                    tokens = "".split_ascii_whitespace();
                    s
                },
            },
            "join" => JoinRoom,
            "start" => StartGame,
            _ => MakeMove(s.parse()?),
        };
        if !matches!(mov, MakeMove(_)) && tokens.next().is_some() {
            return Err("Trailing tokens");
        }
        Ok(mov)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub enum Response<Game: GameT> {
    NotLoggedIn,
    RoomList(Vec<Room<Game>>),
    Room(Room<Game>),
    Error(String),
}

impl<Game: GameT> Display for Response<Game> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Response::NotLoggedIn => write!(f, "Please log in: login <username>"),
            Response::Error(err) => write!(f, "Error: {err}"),
            Response::RoomList(rooms) => {
                writeln!(f, "Rooms:")?;
                for room in rooms {
                    writeln!(f, " {room}")?;
                }
                Ok(())
            }
            Response::Room(room) => writeln!(f, "{room}"),
        }
    }
}

// server-only implementations

impl<Game: GameT> RoomState<Game> {
    pub fn make_move(&mut self, userid: &String, mov: Game::Move) -> Result<(), &'static str> {
        match self {
            RoomState::WaitingForPlayers { .. } => Err("Game did not start yet"),
            RoomState::Started(g) => {
                let g = g.as_mut().unwrap();
                g.make_move(userid, mov)
            }
            RoomState::Ended(_) => Err("Game already finished"),
        }
    }
}

impl<Game: GameT> Room<Game> {
    pub fn to_list_item(&self) -> Self {
        Self {
            roomid: self.roomid,
            settings: self.settings.clone(),
            players: self.players.clone(),
            state: match &self.state {
                RoomState::Started(_) => RoomState::Started(None),
                RoomState::Ended(_) => RoomState::Ended(None),
                s => s.clone(),
            },
        }
    }
    pub fn to_view(&self, userid: &UserId) -> Self {
        Self {
            roomid: self.roomid,
            settings: self.settings.clone(),
            players: self.players.clone(),
            state: match &self.state {
                RoomState::Started(g) => RoomState::Started(g.as_ref().map(|g| g.to_view(userid))),
                s => s.clone(),
            },
        }
    }

    pub fn start_game(&mut self) {
        let RoomState::WaitingForPlayers {..} = self.state else {
            return;
        };
        self.state =
            RoomState::Started(Some(Game::new(self.players.clone(), self.settings.clone())));
    }
}
