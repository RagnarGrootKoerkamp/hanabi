use futures_util::{future, pin_mut, StreamExt};
use hanabi_base::Game;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_util::codec::{FramedRead, LinesCodec};
use turnbased_game_server::{Action, Response};

#[tokio::main]
async fn main() {
    let (stdin_sink, stdin_stream) = futures_channel::mpsc::unbounded();

    tokio::spawn(read_user_input(stdin_sink));

    //let url = "ws://crew.ragnargrootkoerkamp.nl/websocket/";
    let url = "ws://127.0.0.1:9284";
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");

    let (outgoing, incoming) = ws_stream.split();
    let stdin_to_ws = stdin_stream.map(Ok).forward(outgoing);

    let ws_to_stdout = incoming.for_each(|message| async {
        if let Err(err) = message {
            eprintln!("Error: {err}");
            return;
        }
        let msg = message.unwrap();
        if !msg.is_binary() {
            return;
        }
        let text = msg.into_data();
        let response: Response<hanabi_base::Game> = serde_json::from_slice(&text).unwrap();
        eprintln!("{response}");
        eprint!("action:\n ");
    });

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;

    // This is needed to kill the hanging stdin task.
    std::process::exit(0);
}

async fn read_user_input(tx: futures_channel::mpsc::UnboundedSender<Message>) {
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
                    eprintln!("{err}");
                    eprint!("action: HELP HERE\n ");
                }
            }
        };

        let message = Message::Binary(serde_json::to_vec(&action).unwrap());
        tx.unbounded_send(message).unwrap();
    }
}
