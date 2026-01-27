use crate::Message;
use std::{os::linux::raw::stat, sync::Arc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use tokio::{io::{self, AsyncBufReadExt, BufReader}, sync::Mutex};
use pluton_core::{cryptography::{get_signing_key, sign_message}, helper, networking::definitions::{self, TextNetworkMessage}};
use chrono::{Local, Utc, TimeZone};
use tokio_tungstenite::tungstenite::{client, http::status};

pub async fn handle_incoming(message: Message, client_state: Arc<Mutex<definitions::ClientState>>) {
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

pub async fn print_messages(client_state: Arc<Mutex<definitions::ClientState>>) {
    let client_lock = client_state.lock().await;
    let messages = client_lock.current_messages.clone();
    let peers = client_lock.peers.clone();
    drop(client_lock);
    
    for text_msg in messages {
        let sender_username = match peers.get(&text_msg.sender) {
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

pub async fn manage_request(msg: TextNetworkMessage, client_state: Arc<Mutex<definitions::ClientState>>) {
    match msg {
        definitions::TextNetworkMessage::ServerText(text_msg) => {
            match pluton_core::client::incoming::receive_server_text(text_msg.clone(), client_state).await {
                Ok((datetime, sender_username)) => {
                    println!("{} - {}: {}", datetime, sender_username, text_msg.plaintext);
                },
                Err(e) => { eprintln!("{}", e); }
            };
        }
        definitions::TextNetworkMessage::ServerStatus(server_status) => {
            println!("Welcome!");
            print!("\x1B[2J\x1B[1;1H"); // Clear terminal
            if let Err(e) = pluton_core::client::incoming::receive_server_status(server_status, client_state.clone()).await {
                eprintln!("{}", e);
            }
            print_messages(client_state).await;
            
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
        definitions::TextNetworkMessage::UserStatusChange(status_change) => {
            let mut client_lock = client_state.lock().await;

            if let Some(peer) = client_lock.peers.get_mut(&status_change.public_key) {
                peer.status = status_change.status;
            }

            drop(client_lock);
        }
        definitions::TextNetworkMessage::UserJoin(join_info) => {
            let mut client_lock = client_state.lock().await;

            let peer = definitions::Peer {
                username: join_info.username,
                address: join_info.address,
                status: definitions::UserStatus::Online,
                roles: vec![]
            };

            client_lock.peers.insert(join_info.public_key, peer);
            drop(client_lock);
        }
        _ => { } // Client messages are ignored
    }
}

pub async fn manage_commands(
    trimmed_message: &str,
    client_state: Arc<Mutex<definitions::ClientState>>,
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
            match trimmed_message.split_whitespace().next() {
                Some("/swap") => {
                    let args: Vec<&str> = trimmed_message.split_whitespace().collect();

                    let client_lock = client_state.lock().await;
                    let message_channels = client_lock.message_channels.clone();
                    drop(client_lock);

                    if args.len() != 2 {
                        println!("Invalid command");
                        return
                    }

                    for channel in message_channels {
                        if args[1].to_string() == channel.name {
                            let mut client_lock = client_state.lock().await;
                            client_lock.current_channel = channel.clone();
                            drop(client_lock);

                            let message_request = definitions::TextNetworkMessage::ClientRequestMessages(
                                definitions::ClientRequestMessages { 
                                    range: 0..255,
                                    channel: channel.clone()
                                }
                            );

                            tx.unbounded_send(Message::Text(serde_json::to_string(&message_request).expect("couldnt convert").into())).unwrap();
                            print!("\x1B[2J\x1B[1;1H"); // Clear terminal when changing channels
                            println!("You are now in: #{}", channel.name);
                        }
                    }
                }
                Some("/kick") => {
                    let args: Vec<&str> = trimmed_message.split_whitespace().collect();
                    
                    if args.len() < 3 {
                        println!("Incorrect amount of arguements (/kick user reason)");
                        return
                    }

                    let client_lock = client_state.lock().await;
                    let peers = client_lock.peers.clone();
                    drop(client_lock);

                    let Some(recipient) = peers
                        .iter()
                        .find(|(_, v)| v.username == args[1])
                        .map(|(k, _)| k)
                    else {
                        println!("User not found");
                        return
                    };

                    let reason = args[2..].join(" ");

                    let kick_request = definitions::TextNetworkMessage::ClientKickRequest(
                        definitions::ClientKickRequest {
                            recipient: *recipient,
                            reason
                        }
                    );

                    tx.unbounded_send(Message::Text(serde_json::to_string(&kick_request).expect("couldnt convert").into())).unwrap();
                }
                _ => {
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
