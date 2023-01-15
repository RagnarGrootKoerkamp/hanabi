use crate::types::{Action, Response};
use crate::GameT;
use futures_util::{future, pin_mut, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_util::codec::{FramedRead, LinesCodec};

pub async fn start_client<Game: GameT>(address: &str) {
    let (stdin_sink, stdin_stream) = futures_channel::mpsc::unbounded();

    let (ws_stream, _) = connect_async(address).await.expect("Failed to connect");

    tokio::spawn(read_user_input::<Game>(stdin_sink));

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
        eprint!("action:\n ");
    });

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
}

async fn read_user_input<Game: GameT>(tx: futures_channel::mpsc::UnboundedSender<Message>) {
    let stdin = tokio::io::stdin();
    let mut lines = FramedRead::new(stdin, LinesCodec::new());
    loop {
        let action: Action<Game> = loop {
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
                    eprintln!(" action (lobby): login <username> | logout | enter <roomid> | new <min> <max> <settings>");
                    eprintln!(" action (game):  join | leave | start");
                    eprintln!(" move   (game):  {}", Game::move_help());
                    eprint!(" ");
                }
            }
        };

        let message = Message::Binary(serde_json::to_vec(&action).unwrap());
        tx.unbounded_send(message).unwrap();
    }
}
