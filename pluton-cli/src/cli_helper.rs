use crate::{ClientState, Peer, Message};
use std::{sync::Arc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use tokio::{io::{self, AsyncBufReadExt, BufReader}, sync::Mutex};
use pluton_core::{cryptography::get_signing_key, networking::definitions};
use tokio_tungstenite::tungstenite::handshake::server;
use chrono::{Local, Utc, TimeZone};

pub async fn handle_incoming(message: Message, client_state: Arc<Mutex<ClientState>>) {
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
                    let sender_username = match client_state.lock().await.peers.get(&text_msg.sender) {
                        Some(peer) => {
                            peer.username.clone()
                        }
                        None => String::from("Error")
                    };

                    let datetime = Utc.timestamp_opt(text_msg.timestamp, 0)
                        .single()
                        .expect("Invalid timestamp")
                        .with_timezone(&Local);

                    println!("{} - {}: {}", datetime, sender_username, text_msg.plaintext);
                }
                definitions::TextNetworkMessage::ServerStatus(server_status) => {
                    println!("Welcome!");

                    {
                        let mut state_lock = client_state.lock().await;
                        // Let's get some peers
                        for user in server_status.users {
                            state_lock.peers.insert(
                                user.public_key,
                                Peer {
                                    username: user.username,
                                    address: user.address
                                }
                            );
                        }

                        println!("{:?}", state_lock.peers);
                    }

                    for text_msg in server_status.messages {
                        let sender_username = match client_state.lock().await.peers.get(&text_msg.sender) {
                            Some(peer) => {
                                peer.username.clone()
                            }
                            None => String::from("Error")
                        };

                        let datetime = Utc.timestamp_opt(text_msg.timestamp, 0)
                            .single()
                            .expect("Invalid timestamp")
                            .with_timezone(&Local);

                        println!("{} - {}: {}", datetime, sender_username, text_msg.plaintext);
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

pub async fn login(args: Vec<String>) -> Option<SigningKey> {
    if let [_, command, username, password] = args.as_slice() {
        if command.trim() == "--create_account" {
            pluton_core::account_management::sign_up(username.to_string(), password.to_string()).await;
            return None
        }
    }

    if !pluton_core::account_management::check_account_exists().await {
        println!("An account is required! Create one with --create_account [USERNAME] [PASSWORD]");
        return None
    }

    println!("What is your password? ");

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    let raw_password = lines.next_line().await.ok()?.unwrap_or_default();

    let user_password = raw_password.trim();

    // Now connecting to server

    let signing_key = get_signing_key(user_password).await.expect("im too lazy to handle errors");

    Some(signing_key)
}