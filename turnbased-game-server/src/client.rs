use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::types::{Action, Response, Room, UserId};
use crate::GameT;
use futures_util::{future, pin_mut, StreamExt};
use owo_colors::OwoColorize;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_util::codec::{FramedRead, LinesCodec};

struct ClientState<Game: GameT> {
    userid: Option<UserId>,
    room: Option<Room<Game>>,
}

impl<Game: GameT> Default for ClientState<Game> {
    fn default() -> Self {
        Self {
            userid: Default::default(),
            room: Default::default(),
        }
    }
}

pub enum ClientOrServerAction<Game: GameT> {
    ServerAction(Action<Game>),
    ClientAction(Game::ClientAction),
}

impl<Game: GameT> FromStr for ClientOrServerAction<Game> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = match s.parse() {
            Ok(action) => return Ok(ClientOrServerAction::ClientAction(action)),
            Err(err) => err,
        };
        match s.parse() {
            Ok(action) => Ok(ClientOrServerAction::ServerAction(action)),
            Err(err2) => Err(if err != "Unknown action" { err } else { err2 }),
        }
    }
}

pub async fn start_client<Game: GameT>(address: &str) {
    let (stdin_sink, stdin_stream) = futures_channel::mpsc::unbounded();

    let (ws_stream, _) = connect_async(address).await.expect("Failed to connect");

    let state: Arc<Mutex<ClientState<Game>>> = Arc::new(Mutex::new(ClientState::default()));

    tokio::spawn(read_user_input::<Game>(stdin_sink, state.clone()));

    let (outgoing, incoming) = ws_stream.split();
    let stdin_to_ws = stdin_stream.map(Ok).forward(outgoing);

    let ws_to_stdout = incoming.for_each(|msg| async {
        let msg = msg
            .map_err(|err| {
                eprintln!("Error: {err}");
                // Kill the hanging stdin task.
                std::process::exit(1);
            })
            .unwrap();
        if !msg.is_binary() {
            return;
        }
        let text = msg.into_data();
        let response: Response<Game> = serde_json::from_slice(&text).unwrap();

        eprint!("{response}");
        match response {
            Response::LoggedIn(userid) => {
                state.lock().unwrap().userid = Some(userid.clone());
                state.lock().unwrap().room = None;
                // The login message is followed by another message anyway.
            }
            Response::Room(room) => {
                state.lock().unwrap().room = Some(room);
                eprint!("{}", "action: ".bold());
                eprint!("{}", 7 as char);
            }
            _ => {
                state.lock().unwrap().room = None;
                eprint!("{}", "action: ".bold());
            }
        };
    });

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
}

async fn read_user_input<Game: GameT>(
    tx: futures_channel::mpsc::UnboundedSender<Message>,
    state: Arc<Mutex<ClientState<Game>>>,
) {
    let stdin = tokio::io::stdin();
    let mut lines = FramedRead::new(stdin, LinesCodec::new());
    loop {
        let action: ClientOrServerAction<Game> = loop {
            let line = lines.next().await;
            let Some(line) = line else {
                return;
            };
            if line.is_err() {
                continue;
            }
            match line.unwrap().parse() {
                Ok(action) => break action,
                Err(err) => {
                    eprintln!("Error: {err}");
                    eprintln!("Possible actions:");
                    eprintln!(" action (lobby): login <username> | logout | new <min> <max> <settings> | join <roomid> | watch <roomid>");
                    eprintln!(" action (game):  join | leave | start");
                    eprintln!(" move   (game):  {}", Game::move_help());
                    eprint!(" ");
                }
            }
        };

        match action {
            ClientOrServerAction::ServerAction(action) => {
                let message = Message::Binary(serde_json::to_vec(&action).unwrap());
                tx.unbounded_send(message).unwrap();
            }
            ClientOrServerAction::ClientAction(action) => {
                if let Some(room) = &mut state.lock().unwrap().room {
                    match &mut room.state {
                        crate::types::RoomState::WaitingForPlayers { .. } => {
                            eprintln!(" Error: {}", "Game didn't start yet".bold())
                        }
                        crate::types::RoomState::Started(Some(game))
                        | crate::types::RoomState::Ended(Some(game)) => {
                            game.do_client_action(action);
                        }
                        _ => unreachable!("Game should be set."),
                    }
                } else {
                    eprintln!(" Error: {}", "Not in a room".bold());
                }
                eprint!("{}", "action: ".bold());
            }
        };
    }
}
