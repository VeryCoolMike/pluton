// The is a very simple server for debugging and development purposes.
// It is probably unsafe, crashes when fed incorrect data, poorly written, and might not reflect the pluton protocol.
// Do not use this.

use std::env;
use std::io;

use ed25519_dalek::VerifyingKey;
use futures_util::{future, pin_mut, StreamExt, SinkExt};
use pluton_core::cryptography::get_signing_key;
use pluton_core::cryptography::sign_message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use ed25519_dalek::SigningKey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use pluton_core::networking::definitions;
use pluton_core::helper;
mod cli_helper;

struct Peer {
    username: String
}

struct ClientState {
    peers: Vec<HashMap<VerifyingKey, Peer>>,
    current_message_id: u32,
    signing_key: SigningKey
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if let [_, command, username, password] = args.as_slice() {
        if command.trim() == "--create_account" {
            pluton_core::account_management::sign_up(username.to_string(), password.to_string()).await;
            return
        }
    }

    if !pluton_core::account_management::check_account_exists().await {
        println!("An account is required! Create one with --create_account [USERNAME] [PASSWORD]");
        return
    }

    println!("What is your password? ");
    let mut raw_password = String::new();
    io::stdin()
        .read_line(&mut raw_password)
        .expect("Failed to read line");
    let user_password = raw_password.trim();

    // Now connecting to server

    let signing_key = get_signing_key(user_password).await.expect("im too lazy to handle errors");

    let client_state = Arc::new(Mutex::new(ClientState {
        peers: vec![],
        current_message_id: 0,
        signing_key: signing_key
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
    }
    
    // Authenticated with the server

    let stdin_to_ws = stdin_rx.map(Ok).forward(outgoing);
    let ws_to_stdout = {
        incoming.for_each(|message| async {
            handle_incoming(message.unwrap(), client_state.clone()).await;
        })
    };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::join(stdin_to_ws, ws_to_stdout).await;
}

async fn handle_incoming(message: Message, client_state: Arc<Mutex<ClientState>>) {
    println!("Received: {:?}", message.to_text().unwrap());
    match message {
        Message::Text(text) => {
            let msg: definitions::TextNetworkMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Invalid message");
                    return;
                }
            };

            match msg {
                definitions::TextNetworkMessage::ServerText(text_msg) => {
                    println!("{} - {:?}: {}", text_msg.timestamp, text_msg.sender, text_msg.plaintext);
                }
                definitions::TextNetworkMessage::ServerStatus(server_status) => {
                    println!("Welcome!");
                    for text_msg in server_status.messages {
                        println!("{} - {:?}: {}", text_msg.timestamp, text_msg.sender, text_msg.plaintext);
                    }
                }
                _ => { } // Client messages are ignored
            }
        }
        _ => { 
            println!("Not implemented");
        } // TODO!
    }
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
async fn read_stdin(tx: futures_channel::mpsc::UnboundedSender<Message>, client_state: Arc<Mutex<ClientState>>) {
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

        let signing_key = &client_state.lock().await.signing_key;

        let text_message = definitions::TextNetworkMessage::ClientText(
            definitions::ClientTextMessage {
                plaintext: string_message.clone(),
                signed_message: sign_message(&string_message, &signing_key).await,
                id: client_state.lock().await.current_message_id
            }
        );
        println!("Sending: {:?}", text_message);
        tx.unbounded_send(Message::Text(serde_json::to_string(&text_message).expect("couldnt convert").into())).unwrap();
        client_state.lock().await.current_message_id += 1;
    }
}
