// The is a very simple client for debugging and development purposes.
// It is probably unsafe, crashes when fed incorrect data, poorly written, and might not reflect the pluton protocol.
// Do not use this.

use std::env;

use futures_util::{future, pin_mut, StreamExt, SinkExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use pluton_core::networking::definitions;
use pluton_core::helper;

mod cli_helper;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let signing_key = match cli_helper::login(args).await {
        Some(key) => key,
        None => return,
    };
       
    let client_state = Arc::new(Mutex::new(definitions::ClientState {
        peers: HashMap::new(),
        current_message_id: 0,
        signing_key: signing_key,
        current_channel: definitions::Channel {id: 0, name: String::from("general") },
        current_messages: vec![],
        message_channels: vec![],
        voice_channels: vec![]
    }));

    let url = env::args().nth(1).unwrap_or_else(|| String::from("ws://127.0.0.1:6767"));

    let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
    tokio::spawn(read_stdin(stdin_tx.clone(), client_state.clone()));

    let (ws_stream, _) = connect_async(&url).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");

    let (mut outgoing, mut incoming) = ws_stream.split(); // SplitSink and SplitStream

    let cfg: pluton_core::account_management::Settings = if let Ok(settings) = confy::load("pluton", None) {
        settings
    } else {
        eprintln!("[cli] Unable to load settings");
        panic!("[cli] Unable to load settings");
    };

    let username = cfg.username;

    let public_key = if let Ok(key) = helper::general::vec_to_verifying_key(helper::base64::from_base64(cfg.verifying_key)) {
        key
    } else {
        eprintln!("Failed to get verifying key");
        panic!("Failed to get verifying key");
    };

    let handshake_result = pluton_core::networking::auth_handshake::auth_handshake_client(
        &mut outgoing,
        &mut incoming,
        &username,
        public_key,
        String::new(),
        client_state.lock().await.signing_key.clone()
    ).await;

    if handshake_result == Ok(definitions::HandshakeStatus::Complete) {
        println!("Successful handshake");
    } else {
        println!("Could not complete handshake: {:?}", handshake_result);
        return;
    }
    
    // Authenticated with the server

    let stdin_to_ws = stdin_rx.map(Ok).forward(outgoing);
    let ws_to_stdout = {
        incoming.for_each(|message| async {
            cli_helper::handle_incoming(message.unwrap(), client_state.clone()).await;
        })
    };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::join(stdin_to_ws, ws_to_stdout).await;
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
async fn read_stdin(
    tx: futures_channel::mpsc::UnboundedSender<Message>,
    client_state: Arc<Mutex<definitions::ClientState>>
) {
    let mut stdin = tokio::io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf).await {
            Err(_) | Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);

        // Text Message Construction

        let string_message = String::from_utf8(buf.clone()).expect("stop yapping");
        let trimmed_message = string_message.trim();

        // Check for commands

        cli_helper::manage_commands(trimmed_message, client_state.clone(), tx.clone()).await;
    }
}
