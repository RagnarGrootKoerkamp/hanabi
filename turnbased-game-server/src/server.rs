use super::*;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use hanabi_base::GameT;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::protocol::Message;

struct User {
    //userid: UserId,
    // TODO: Fill this
    //rooms: Vec<RoomId>,
    sockets: Vec<ClientId>,
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
    roomid: Option<RoomId>,
}

pub struct ServerState<Game: GameT> {
    /// All users in the server.
    users: HashMap<UserId, User>,
    /// All rooms in the server.
    rooms: Vec<(Room<Game>, Vec<ClientId>)>,
    /// All currently open sockets.
    clients: HashMap<ClientId, Client>,
}

#[derive(Clone)]
pub struct Server<Game: GameT> {
    state: Arc<Mutex<ServerState<Game>>>,
}

impl<Game: GameT> ServerState<Game> {
    fn room(&self, roomid: RoomId) -> &Room<Game> {
        &self.rooms[roomid.0].0
    }
    fn room_mut(&mut self, roomid: RoomId) -> &mut Room<Game> {
        &mut self.rooms[roomid.0].0
    }

    fn watchers(&self, roomid: RoomId) -> &Vec<ClientId> {
        &self.rooms[roomid.0].1
    }
    fn watchers_mut(&mut self, roomid: RoomId) -> &mut Vec<ClientId> {
        &mut self.rooms[roomid.0].1
    }

    fn client(&self, clientid: ClientId) -> &Client {
        self.clients.get(&clientid).unwrap()
    }
    fn client_mut(&mut self, clientid: ClientId) -> &mut Client {
        self.clients.get_mut(&clientid).unwrap()
    }

    fn room_list(&self) -> Response<Game> {
        Response::RoomList(
            self.rooms
                .iter()
                .map(|room| room.0.to_list_item())
                .collect(),
        )
    }

    fn handle_action(
        &mut self,
        clientid: ClientId,
        action: Action<Game>,
    ) -> Option<Response<Game>> {
        use Response::*;

        match action {
            Action::Login(login_userid) => {
                self.logout(clientid);
                self.clients.get_mut(&clientid).unwrap().userid = Some(login_userid.clone());
                self.users.insert(login_userid, User { sockets: vec![] });
                return Some(self.room_list());
            }
            Action::Logout => {
                self.logout(clientid);
                return Some(NotLoggedIn);
            }
            Action::LeaveRoom => {
                self.leave_room(clientid);
                return Some(self.room_list());
            }
            _ => {}
        };

        // Remaining actions require a user to be logged in.
        let &Client { userid, roomid, .. } = &self.client(clientid);
        let Some(userid) = userid.clone() else {
            return Some(NotLoggedIn);
        };

        match action {
            Action::NewRoom {
                min_players,
                max_players,
                settings,
            } => {
                let roomid = RoomId(self.rooms.len());
                self.rooms.push((
                    crate::Room {
                        roomid,
                        settings,
                        players: vec![userid.clone()],
                        state: RoomState::WaitingForPlayers {
                            min_players,
                            max_players,
                        },
                    },
                    vec![clientid],
                ));
                self.leave_room(clientid);
                self.client_mut(clientid).roomid = Some(roomid);
                return Some(Room(self.room(roomid).to_view(&userid)));
            }
            Action::WatchRoom(roomid) => {
                self.watch_room(clientid, roomid);
                return Some(Room(self.room(roomid).to_view(&userid)));
            }
            _ => {}
        }

        // Remaining actions act on a room.
        let Some(roomid) = *roomid else {
            return None;
        };

        match action {
            Action::JoinRoom => {
                let room = self.room_mut(roomid);
                let RoomState::WaitingForPlayers { max_players, .. } = room.state else {
                    return Some(Error("Room is not waiting for players".into()));
                };
                if room.players.iter().find(|&x| x == &userid).is_some() {
                    return Some(Error("User is already in room".into()));
                }
                if room.players.len() == max_players {
                    return Some(Error("Room is already full".into()));
                }
                room.players.push(userid.clone());
                if room.players.len() == max_players {
                    if let Err(err) = self.start_game(&userid, roomid) {
                        return Some(Error(err.into()));
                    }
                }
                roomid
            }
            Action::StartGame => {
                if let Err(err) = self.start_game(&userid, roomid) {
                    return Some(Error(err.into()));
                }
                roomid
            }
            Action::MakeMove(mov) => {
                let room = self.room_mut(roomid);
                if !room.players.contains(&userid) {
                    return Some(Error("User did not join room".into()));
                }
                if let Err(err) = room.state.make_move(&userid, mov) {
                    return Some(Error(err.into()));
                }

                roomid
            }
            _ => unreachable!(),
        };

        let room = self.room(roomid);
        for watching_client in self.watchers(roomid) {
            let client = self.client(*watching_client);
            client
                .sink
                .send(Room(room.to_view(client.userid.as_ref().unwrap())));
        }
        // Client is already updated in the loop above.
        None
    }

    fn watch_room(&mut self, clientid: std::net::SocketAddr, roomid: RoomId) {
        self.leave_room(clientid);
        self.client_mut(clientid).roomid = Some(roomid);
        self.watchers_mut(roomid).push(clientid);
    }

    fn disconnect(&mut self, clientid: std::net::SocketAddr) {
        eprintln!("{} disconnected", &clientid);
        let Client { userid, roomid, .. } = self.clients.remove(&clientid).unwrap();
        if let Some(room) = roomid {
            self.watchers_mut(room).retain(|x| x != &clientid);
        }
        if let Some(userid) = userid {
            self.users
                .get_mut(&userid)
                .unwrap()
                .sockets
                .retain(|x| x != &clientid);
        }
    }

    fn connect(&mut self, clientid: std::net::SocketAddr, sink: Sink) {
        eprintln!("{} connected", &clientid);
        self.clients.insert(
            clientid,
            Client {
                sink: sink.clone(),
                userid: None,
                roomid: None,
            },
        );
        sink.send(Response::<Game>::NotLoggedIn);
    }

    fn start_game(&mut self, userid: &UserId, roomid: RoomId) -> Result<(), &'static str> {
        let room = self.room_mut(roomid);
        if !room.players.contains(&userid) {
            Err("User did not join room")
        } else {
            Ok(room.start_game())
        }
    }

    fn leave_room(&mut self, clientid: ClientId) {
        if let Some(roomid) = self.clients.get(&clientid).unwrap().roomid {
            self.watchers_mut(roomid).retain(|x| x != &clientid);
            self.clients.get_mut(&clientid).unwrap().roomid = None;
        }
    }

    fn logout(&mut self, clientid: ClientId) {
        self.leave_room(clientid);
        // Disassociate the user from the client.
        let userid = &mut self.clients.get_mut(&clientid).unwrap().userid;
        if let Some(loggedin_userid) = userid {
            self.users
                .get_mut(loggedin_userid)
                .unwrap()
                .sockets
                .retain(|x| x != &clientid);
            *userid = None;
        }
    }
}

impl<Game: GameT> Server<Game> {
    pub async fn start(address: &str) {
        let server = Server::<hanabi_base::Game>::new();
        let listener = TcpListener::bind(&address).await.unwrap();
        while let Ok((stream, clientid)) = listener.accept().await {
            tokio::spawn(server.clone().handle_connection(stream, clientid));
        }
    }

    fn new() -> Self {
        Server {
            state: Arc::new(Mutex::new(ServerState {
                users: Default::default(),
                rooms: Default::default(),
                clients: Default::default(),
            })),
        }
    }

    async fn handle_connection(self, raw_stream: TcpStream, clientid: ClientId) {
        let ws_stream = tokio_tungstenite::accept_async(raw_stream)
            .await
            .expect("Error during the websocket handshake occurred");
        println!("WebSocket connection established: {}", clientid);

        // Write and read part of the websocket stream.
        let (ws_outgoing, ws_incoming) = ws_stream.split();

        // Internal MPSC channel to handle buffering and flushing of messages to the websocket.
        let (sink, internal_stream) = unbounded();
        // Forward messages to the internal queue to the websocket.
        let receive_from_others = internal_stream.map(Ok).forward(ws_outgoing);

        // Wrap the internal sink to accept Action.
        let sink = Sink(sink);
        self.state.lock().unwrap().connect(clientid, sink);

        // Process all incoming messages on this websocket.
        let handle_incoming = ws_incoming.try_for_each(|msg| {
            match serde_json::from_slice(&msg.into_data()) {
                Ok(action) => self.handle_action(clientid, action),
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

        self.state.lock().unwrap().disconnect(clientid);
    }

    fn handle_action(&self, clientid: ClientId, action: Action<Game>) {
        let server = &mut self.state.lock().unwrap();
        if let Some(response) = server.handle_action(clientid, action) {
            server.client(clientid).sink.send(response);
        }
    }
}
