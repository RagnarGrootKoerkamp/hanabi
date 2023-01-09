pub mod server;

use hanabi_base::GameT;
use serde::{Deserialize, Serialize};

// TODO: Separate Player id and name. For now the name is the id.
pub type UserId = String;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct RoomId(pub usize);

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
    NewRoom,
    /// Join the current room if it is waiting for players.
    JoinRoom,

    /// Start the game in the current room.
    StartGame,

    /// Make a move in the current room.
    MakeMove(Game::Move),
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub enum Response<Game: GameT> {
    NotLoggedIn,
    RoomList(Vec<Room<Game>>),
    Room(Room<Game>),
    Error(String),
}

// implementations

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
