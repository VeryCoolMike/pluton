use crate::{ClientState, Peer, Message};
use std::{sync::Arc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use tokio::{io::{self, AsyncBufReadExt, BufReader}, sync::Mutex};
use pluton_core::{cryptography::{get_signing_key, sign_message}, networking::definitions::{self, TextNetworkMessage}};
use chrono::{Local, Utc, TimeZone};
use tokio_tungstenite::tungstenite::handshake::server;

pub async fn handle_incoming(message: Message, client_state: Arc<Mutex<ClientState>>) {
    //println!("Received: {:?}", message.to_text().unwrap());
    match message {
        Message::Text(text) => {
            let msg: definitions::TextNetworkMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Invalid message {e} {text}");
                    return;
                }
            };

            manage_request(msg, client_state).await; 
        }
        _ => { 
            println!("Not implemented");
        } // TODO!
    }
}

pub async fn manage_request(msg: TextNetworkMessage, client_state: Arc<Mutex<ClientState>>) {
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
                let mut client_lock = client_state.lock().await;
                // Let's get some peers
                for user in server_status.users {
                    client_lock.peers.insert(
                        user.public_key,
                        Peer {
                            username: user.username,
                            address: user.address,
                            roles: user.roles,
                            status: user.status
                        }
                    );
                }

                client_lock.current_channel = server_status.default_channel;
                client_lock.message_channels = server_status.message_channels;
                client_lock.voice_channels = server_status.voice_channels;
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
        definitions::TextNetworkMessage::ServerRequestMessages(response) => {
            for text_msg in response.messages {
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

pub async fn manage_commands(
    trimmed_message: &str,
    client_state: Arc<Mutex<ClientState>>,
    tx: futures_channel::mpsc::UnboundedSender<Message>
) {
    match trimmed_message {
        "/list" => {
            let client_lock = client_state.lock().await;
            let current_channel = client_lock.current_channel.clone();
            let message_channels = client_lock.message_channels.clone();
            let peers = client_lock.peers.clone();
            drop(client_lock);

            println!("]--channels--[");
            println!("You are in #{}", current_channel.name);
            for channel in message_channels {
                if current_channel.id == channel.id {
                    println!("> #{} - text", channel.name);
                } else {
                    println!("#{} - text", channel.name);
                }
            }
            println!("]------------[\n");

            println!("]----peers----[");
            println!("--username (address): status--");
            for peer in peers {
                println!("{} ({}): {:?}", peer.1.username, peer.1.address, peer.1.status);
            }
            println!("]------------[")
        }
        _ => {
            if trimmed_message.starts_with("/swap") {
                let args: Vec<&str> = trimmed_message.split(' ').collect();

                let client_lock = client_state.lock().await;
                let message_channels = client_lock.message_channels.clone();
                drop(client_lock);

                if args.len() != 2 {
                    println!("Invalid command");
                    return
                }

                for channel in message_channels {
                    if args[1].to_string() == channel.name {
                        println!("Found channel");
                        let mut client_lock = client_state.lock().await;
                        client_lock.current_channel = channel.clone();
                        drop(client_lock);

                        let message_request = definitions::TextNetworkMessage::ClientRequestMessages(
                            definitions::ClientRequestMessages { 
                                range: 0..255,
                                channel: channel
                            }
                        );

                        tx.unbounded_send(Message::Text(serde_json::to_string(&message_request).expect("couldnt convert").into())).unwrap();

                        println!("changed channel");
                    }
                }

                return
            }
            let mut client_lock = client_state.lock().await;
            let signing_key = client_lock.signing_key.clone();
            let current_id = client_lock.current_message_id;
            let current_channel = client_lock.current_channel.clone();
            client_lock.current_message_id += 1;
            drop(client_lock);

            let text_message = definitions::TextNetworkMessage::ClientText(
                definitions::ClientTextMessage {
                    plaintext: trimmed_message.to_string(),
                    signed_message: sign_message(&trimmed_message, &signing_key).await,
                    id: current_id,
                    channel: current_channel
                }
            );
            //println!("sending: {:?}", text_message);
            tx.unbounded_send(Message::Text(serde_json::to_string(&text_message).expect("couldnt convert").into())).unwrap();

        }
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