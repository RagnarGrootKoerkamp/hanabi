
#![allow(dead_code)]
#![allow(unused_variables)]

use crew_lib::crew;
use crew_lib::crew::{no_premove, no_premove_err, DoPremove};
use crew_lib::first;
use crew_lib::pos_of;
use crew_lib::second;
use crew_lib::types::*;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::protocol::Message;


#[derive(Debug, Serialize, Deserialize)]
enum ServerState {
    // The players joined so far and their status.
    WaitingForPlayers(HashMap<SocketAddr, (String, PlayerStatus)>),
    // The game itself.
    Started {
        // Address to name and status.
        names: HashMap<SocketAddr, (String, PlayerStatus)>,
        // Same order as in-game player ids.
        // NOTE: Players are keyed by string, so it's possible to watch and join
        // again with the same name.
        players: Vec<String>,
        game: crew::Crew,
    },
}

impl ServerState {
    fn new() -> ServerState {
        ServerState::WaitingForPlayers(HashMap::new())
    }

    fn player_idx(&self, name: &String) -> Option<crew::Player> {
        match self {
            ServerState::Started { players, .. } => pos_of(players, name),
            _ => unreachable!(),
        }
    }

    fn names(&self) -> &HashMap<SocketAddr, (String, PlayerStatus)> {
        match self {
            ServerState::WaitingForPlayers(players) => players,

            ServerState::Started { names, .. } => names,
        }
    }

    fn name(&self, addr: &SocketAddr) -> Option<&String> {
        self.names().get(addr).map(first)
    }

    fn to_client(&self, addr: &SocketAddr) -> ClientState {
        match self {
            ServerState::WaitingForPlayers(players) => {
                ClientState::WaitingForPlayers(players.values().cloned().collect())
            }
            ServerState::Started {
                names,
                players,
                game,
            } => {
                let name = names.get(addr).map(|(name, _)| name);
                ClientState::Started {
                    players: players.clone(),
                    spectators: names
                        .values()
                        .filter_map(|(name, _)| -> Option<&String> {
                            if players.contains(name) {
                                None
                            } else {
                                Some(name)
                            }
                        })
                        .cloned()
                        .collect(),
                    game: game.view(
                        players
                            .iter()
                            .position(|other_name| name == Some(other_name)),
                    ),
                }
            }
        }
    }
}

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;
type SharedServerState = Arc<Mutex<ServerState>>;

fn handle_action(
    addr: SocketAddr,
    action: Action,
    shared_state: &mut ServerState,
    peer_map: &PeerMap,
) -> crew_lib::crew::StrAndPremoveResult<Option<ActionLog>> {
    match (action.clone(), &mut *shared_state) {
        (Action::Message(message), _) => unreachable!(),
        // Join pending server: set player state and name.
        // Error is name already taken.
        (Action::Join(name), ServerState::WaitingForPlayers(ref mut names)) => {
            if names
                .iter()
                .any(|(key, (other_name, _))| *key != addr && *other_name == name)
            {
                no_premove("There is already another player with this name")
            } else {
                names.insert(addr, (name, PlayerStatus::Joined));
                Ok(None)
            }
        }
        // Join started server: join as spectator unless player with given name is part of the game but missing.
        (
            Action::Join(name),
            ServerState::Started {
                ref mut names,
                ref players,
                ..
            },
        ) => {
            let name_is_missing = players.contains(&name)
                && !names.values().any(|(other_name, _)| *other_name == name);

            if name_is_missing {
                names.insert(addr, (name, PlayerStatus::Joined));
                return Ok(None);
            }

            if names.contains_key(&addr) {
                return match names[&addr] {
                    (_, PlayerStatus::Spectator) => no_premove("Cannot join an active game."),
                    (ref current_name, PlayerStatus::Joined) if *current_name == name => Ok(None),
                    (ref current_name, PlayerStatus::Joined) => {
                        no_premove("Can't change name while game is in progress")
                    }
                };
            }

            // Name is new and not playing, join as spectator.
            names.insert(addr, (name, PlayerStatus::Spectator));
            no_premove("Game is in progress, joined as spectator")
        }
        // Watch pending server.
        (Action::Watch, ServerState::WaitingForPlayers(ref mut names)) => {
            names
                .entry(addr)
                .and_modify(|tup| tup.1 = PlayerStatus::Spectator);
            Ok(None)
        }
        // Watch a started game: Either an error on invariant.
        (Action::Watch, ServerState::Started { names, .. }) => {
            match names
                .get(&addr)
                .map(|(_, status)| *status == PlayerStatus::Joined)
            {
                Some(true) => no_premove("Cannot watch a game you're playing in"),
                _ => Ok(None),
            }
        }
        // Start the game with the given number of missions.
        // All players who are currently Joined will participate.
        (
            Action::GameAction(crew::Action::Start(num_missions)),
            ServerState::WaitingForPlayers(names),
        ) => {
            if names.get(&addr).map(second).cloned() != Some(PlayerStatus::Joined) {
                return no_premove("Only joined players can start a game");
            }
            let name = &names[&addr].0;
            // Make a game with all currently joined players.
            let peers = peer_map.lock().unwrap();
            // TODO: Shuffle players
            let players: Vec<String> = names
                .values()
                .filter_map(|(name, status)| {
                    if *status == PlayerStatus::Joined {
                        Some(name)
                    } else {
                        None
                    }
                })
                .cloned()
                .collect();
            let num_players: usize = players.len();
            let player = pos_of(&players, name).unwrap();
            *shared_state = ServerState::Started {
                names: names.clone(),
                players,
                game: crew::Crew::new(num_players, num_missions).map_err(no_premove_err)?,
            };
            Ok(Some(ActionLog { player, action }))
        }
        // Starting an already started game is always an error.
        (Action::GameAction(crew::Action::Start(_)), _) => no_premove("Game already started"),
        // Do a game action.
        (
            Action::GameAction(game_action),
            ServerState::Started {
                names,
                players,
                ref mut game,
                ..
            },
        ) => {
            let name = &names
                .get(&addr)
                .ok_or(no_premove_err("Must first join the game"))?
                .0;
            let player = pos_of(players, &name).ok_or(no_premove_err("Player not in game"))?;
            if game_action == crew::Action::End {
                *shared_state = ServerState::WaitingForPlayers(names.clone());
            } else {
                game.act(player, game_action)?;
            }
            Ok(Some(ActionLog { player, action }))
        }
        // GameActions can only be done on a started game.
        (Action::GameAction(_), _) => no_premove("Game hasn't started yet"),
    }
}

async fn handle_connection(
    peer_map: PeerMap,
    shared_state: SharedServerState,
    raw_stream: TcpStream,
    addr: SocketAddr,
) {
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Write and read part of MPSC channel to communicate with this task.
    let (tx, rx) = unbounded();
    let tx_self = tx.clone();
    peer_map.lock().unwrap().insert(addr, tx);

    // Write and read part of the websocket stream.
    let (outgoing, incoming) = ws_stream.split();

    let send_response = |tx: &futures_channel::mpsc::UnboundedSender<Message>,
                         response: Response| {
        let message = Message::Text(serde_json::to_string(&response).unwrap());
        tx.unbounded_send(message).unwrap();
    };

    // Send the current state to this client.
    send_response(
        &tx_self,
        Ok(ResponseType::state(
            shared_state.lock().unwrap().to_client(&addr),
        )),
    );

    // Update the game state for all clients.
    let broadcast = |action: Option<ActionLog>, shared_state: &ServerState| {
        // Broadcast the updated game state to everyone.
        let peers = peer_map.lock().unwrap();
        for (peer_addr, tx) in peers.iter() {
            send_response(
                tx,
                Ok(ResponseType::Action {
                    action: action.clone(),
                    state: shared_state.to_client(peer_addr),
                }),
            );
        }
    };

    // Send a message to all players.
    let broadcast_message = |response: MessageResponse| {
        // Broadcast the updated game state to everyone.
        let peers = peer_map.lock().unwrap();
        for (peer_addr, tx) in peers.iter() {
            send_response(tx, Ok(ResponseType::Message(response.clone())));
        }
    };

    let handle_incoming = incoming.try_for_each(|msg| {
        println!("Message: {:?}", msg);
        let text = &msg.into_text();
        if text.is_err() {
            eprintln!("Message is not text: {:?}", text.as_ref().err());
            return future::ok(());
        }
        let action = serde_json::from_str(text.as_ref().unwrap());
        if action.is_err() {
            eprintln!("Couldn't parse message: {:?}", action.err());
            return future::ok(());
        }
        let action = action.unwrap();

        eprintln!("Received action from {}: {:?}", addr, action);

        if let Action::Message(message) = action {
            broadcast_message(MessageResponse {
                user: shared_state
                    .lock()
                    .unwrap()
                    .name(&addr)
                    .cloned()
                    .unwrap_or("anon".to_string()),
                message,
            });
        } else {
            let response =
                handle_action(addr, action, &mut shared_state.lock().unwrap(), &peer_map);

            match response {
                // On Error, notify the requesting player.
                Err((msg, DoPremove(false))) => {
                    send_response(&tx_self, Err(msg.to_string()));
                }
                Err((msg, DoPremove(true))) => {
                    // Send both the error and the updated state containing the premove.
                    send_response(&tx_self, Err(msg.to_string()));
                    send_response(
                        &tx_self,
                        Ok(ResponseType::Action {
                            action: None,
                            state: shared_state.lock().unwrap().to_client(&addr),
                        }),
                    );
                }
                Ok(action) => {
                    broadcast(action, &shared_state.lock().unwrap());
                }
            };
        }

        future::ok(())
    });

    // Forward messages sent to us by other tasks to the client.
    let receive_from_others = rx
        .map(Ok)
        .map(|msg| {
            if let Ok(ref msg) = msg {
                println!(
                    "Send to {}: {:?}",
                    addr,
                    serde_json::from_str::<Response>(msg.to_text().unwrap()).unwrap()
                );
            }
            msg
        })
        .forward(outgoing);

    pin_mut!(handle_incoming, receive_from_others);
    let result = future::select(handle_incoming, receive_from_others).await;
    println!("CONNECTION RESULT {:?}", result);

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
    match *shared_state.lock().unwrap() {
        ServerState::WaitingForPlayers(ref mut names) => names.remove(&addr),
        ServerState::Started { ref mut names, .. } => names.remove(&addr),
    };
}

#[tokio::main]
async fn main() {
    let peer_map = PeerMap::new(Mutex::new(HashMap::new()));
    let shared_state: SharedServerState = Arc::new(Mutex::new(ServerState::new()));

    // Listens for new incoming connections.
    let listener = TcpListener::bind("127.0.0.1:9283").await.unwrap();
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(
            peer_map.clone(),
            shared_state.clone(),
            stream,
            addr,
        ));
    }
}



/// SERVER STATE

// TODO: Separate Player id and name. For now the name is the id.
type Player = String;

enum RoomState<Game> {
    WaitingForPlayers {
        min: usize,
        max: usize,
        players: Vec<Player>,
    },
    Started(Game)
}

struct ServerState {
    rooms: Vec<RoomState>
}
