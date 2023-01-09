#![allow(dead_code)]
#![allow(unused_variables)]

use derivative::Derivative;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use hanabi_base::GameT;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::protocol::Message;

/// SERVER STATE

// TODO: Separate Player id and name. For now the name is the id.
type UserId = String;
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct RoomId(usize);

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound = "")]
enum RoomState<Game: GameT> {
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
struct Room<Game: GameT> {
    id: RoomId,
    settings: Game::Settings,
    players: Vec<UserId>,
    state: RoomState<Game>,
    /// Sockets that are watching this room for updates.
    #[serde(skip)]
    subscribers: Vec<SocketAddr>,
}

impl<Game: GameT> Room<Game> {
    fn to_list_item(&self) -> Self {
        Self {
            id: self.id,
            settings: self.settings.clone(),
            players: self.players.clone(),
            state: match &self.state {
                RoomState::Started(g) => RoomState::Started(None),
                RoomState::Ended(g) => RoomState::Ended(None),
                s => s.clone(),
            },
            subscribers: vec![],
        }
    }
    fn to_view(&self, userid: &UserId) -> Self {
        Self {
            id: self.id,
            settings: self.settings.clone(),
            players: self.players.clone(),
            state: match &self.state {
                RoomState::Started(g) => RoomState::Started(g.as_ref().map(|g| g.to_view(userid))),
                s => s.clone(),
            },
            subscribers: vec![],
        }
    }
}

struct User {
    userid: UserId,
    sockets: Vec<SocketAddr>,
}

#[derive(Clone)]
struct Sink(UnboundedSender<Message>);

impl Sink {
    fn send(&self, response: Response<impl GameT>) {
        let message = Message::Text(serde_json::to_string(&response).unwrap());
        self.0.unbounded_send(message).unwrap();
    }
}

struct Client {
    sink: Sink,
    /// The user who opened the socket.
    userid: Option<UserId>,
    /// The room the socket is watching.
    room: Option<RoomId>,
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
struct ServerState<Game: GameT> {
    /// All users in the server.
    users: HashMap<UserId, User>,
    /// All rooms in the server.
    rooms: Vec<Room<Game>>,
    /// All currently open sockets.
    clients: HashMap<SocketAddr, Client>,
}

/// An action that can be sent over an incoming websocket.
#[derive(Serialize, Deserialize)]
enum Action<Game: GameT> {
    // 'Fake' actions created by the server.
    #[serde(skip)]
    Connect(Sink),
    #[serde(skip)]
    Disconnect,

    /// Which user is using the socket.
    Login(UserId),
    /// User stopped used the socket.
    Logout,

    /// Create a new room.
    NewRoom,
    /// Open and view a room (but do not join it).
    ViewRoom(RoomId),
    /// Stop viewing a room. Tells the server to stop sending updates for the
    /// viewed room.
    LeaveRoom,
    /// Join a room that is waiting for players.
    JoinRoom(RoomId),

    /// Start the game in the room.
    StartGame(RoomId),

    /// Make a move in the given room.
    MakeMove(RoomId, Game::Move),
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
enum Response<Game: GameT> {
    NotLoggedIn,
    LoggedIn(UserId),
    RoomList(Vec<Room<Game>>),
    Room(Room<Game>),
}

#[derive(Clone)]
struct Server<Game: GameT> {
    state: Arc<Mutex<ServerState<Game>>>,
}

impl<Game: GameT> ServerState<Game> {
    fn room_list(&self) -> Response<Game> {
        Response::RoomList(self.rooms.iter().map(|room| room.to_list_item()).collect())
    }

    fn handle_action_locked(
        &mut self,
        addr: SocketAddr,
        action: Action<Game>,
    ) -> Option<Response<Game>> {
        use Response::*;

        match action {
            Action::Connect(sink) => {
                eprintln!("{} connected", &addr);
                self.clients.insert(
                    addr,
                    Client {
                        sink: sink.clone(),
                        userid: None,
                        room: None,
                    },
                );
                return Some(NotLoggedIn);
            }
            Action::Disconnect => {
                eprintln!("{} disconnected", &addr);
                let Client { userid, room, .. } = self.clients.remove(&addr).unwrap();
                if let Some(room) = room {
                    self.rooms[room.0].subscribers.retain(|x| x != &addr);
                }
                if let Some(userid) = userid {
                    self.users
                        .get_mut(&userid)
                        .unwrap()
                        .sockets
                        .retain(|x| x != &addr);
                }
                return None;
            }
            _ => {}
        };

        let Client { sink, userid, room } = &mut self.clients.get_mut(&addr).unwrap();

        match action {
            Action::Login(login_userid) => {
                if userid.is_some() {
                    return Some(self.room_list());
                } else {
                    *userid = Some(login_userid);
                    return Some(self.room_list());
                }
            }
            Action::Logout => {
                // Disassociate the user from the client.
                if let Some(loggedin_userid) = userid {
                    self.users
                        .get_mut(loggedin_userid)
                        .unwrap()
                        .sockets
                        .retain(|x| x != &addr);
                    *userid = None;
                    return Some(NotLoggedIn);
                } else {
                    return Some(NotLoggedIn);
                }
            }
            Action::ViewRoom(roomid) => {}
            Action::LeaveRoom => todo!(),
            Action::JoinRoom(_) => todo!(),
            Action::StartGame(_) => todo!(),
            Action::MakeMove(_, _) => todo!(),
            _ => unreachable!(),
        };
        unreachable!();
    }
}

impl<Game: GameT> Server<Game> {
    fn default() -> Self {
        Server {
            state: Arc::new(Mutex::new(ServerState::default())),
        }
    }

    async fn handle_connection(self, raw_stream: TcpStream, addr: SocketAddr) {
        let ws_stream = tokio_tungstenite::accept_async(raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");
        println!("WebSocket connection established: {}", addr);

        // Write and read part of the websocket stream.
        let (ws_outgoing, ws_incoming) = ws_stream.split();

        // Internal MPSC channel to handle buffering and flushing of messages to the websocket.
        let (sink, internal_stream) = unbounded();
        // Forward messages to the internal queue to the websocket.
        let receive_from_others = internal_stream.map(Ok).forward(ws_outgoing);

        // Wrap the internal sink to accept Action.
        let sink = Sink(sink);
        self.handle_action(addr, Action::Connect(sink));

        // Process all incoming messages on this websocket.
        let handle_incoming = ws_incoming.try_for_each(|msg| {
            match serde_json::from_slice(&msg.into_data()) {
                Ok(action) => self.handle_action(addr, action),
                Err(err) => {
                    eprintln!("Failed to parse message as json: {:?}", err);
                    return future::ok(());
                }
            };
            future::ok(())
        });

        pin_mut!(handle_incoming, receive_from_others);
        let result = future::select(handle_incoming, receive_from_others).await;
        eprintln!("CONNECTION RESULT {:?}", result);

        self.handle_action(addr, Action::Disconnect);
    }

    fn handle_action(&self, addr: SocketAddr, action: Action<Game>) -> Option<Response<Game>> {
        self.state
            .lock()
            .unwrap()
            .handle_action_locked(addr, action)
    }
}

#[tokio::main]
async fn main() {
    let server = Server::<hanabi_base::Game>::default();

    let listener = TcpListener::bind("127.0.0.1:9284").await.unwrap();
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(server.clone().handle_connection(stream, addr));
    }
}
