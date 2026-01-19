use std::{
    collections::HashMap, env, fmt::Binary, net::SocketAddr, sync::Arc, time::{SystemTime, UNIX_EPOCH}
};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{SinkExt, StreamExt, future::{self, join}, pin_mut, stream::TryStreamExt};

use pluton_core::{cryptography::{sign_message, verify_signature}, networking::definitions::{self, UserOverview}};
use crate::server::{database, helper};

use tokio::{net::{TcpListener, TcpStream}, sync::{broadcast, Mutex}};
use tokio_tungstenite::tungstenite::{handshake::server, protocol::Message};

pub async fn handle_connection(
    peer_map: helper::PeerMap,
    raw_stream: TcpStream,
    addr: SocketAddr,
    database: Arc<libsql::Database>,
    server_info: Arc<helper::ServerInfo>
) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    let (mut outgoing, mut incoming) = ws_stream.split();

    let session_token = String::new();

    // Authentication protocol
    let (join_request, _) = match
        pluton_core::networking::auth_handshake::auth_handshake_server(
            &mut outgoing,
            &mut incoming,
            session_token
        ).await
    {
        Ok(v) => v,
        Err(_) => {
            let _ = outgoing.send(Message::Close(None)).await;
            return;
        }
    };

    // At this point, the client has succesfully authenticated themselves

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    
    let info = helper::PeerInfo {
        username: join_request.username,
        tx: tx.clone(),
        public_key: join_request.public_key,
        address: join_request.address,
        roles: vec![],
        status: definitions::UserStatus::Online
    };
    let public_key = info.public_key;
    let info_clone = info.clone();

    let user_exists = match database::user_exists(info_clone.clone(), database.clone()).await {
        Ok(m) => {
            m.is_some()
        },
        Err(_) => return
    };

    if !user_exists && let Err(e) = database::add_user(info_clone, database.clone()).await {
        eprintln!("{e}");
        return
    };

    peer_map.lock().await.insert(addr, info);
    println!("{} has been accepted!", addr);

    let broadcast_recipients: Vec<helper::Tx> = {
        let peers = peer_map.lock().await; 
        peers.iter().filter(|(peer_addr, _)| *peer_addr != &addr).map(|(_, peer)| peer.tx.clone()).collect()
    };

    let join_alert = definitions::TextNetworkMessage::UserStatusChange(
        definitions::UserStatusChange {
            public_key: public_key,
            status: definitions::UserStatus::Online
        }
    );

    for tx in broadcast_recipients {
        tx.unbounded_send(Message::Text(serde_json::to_string(&join_alert).expect("unable to serde").into())).unwrap();
    }

    // Let's give the new client a present! Data!

    let mut users: Vec<UserOverview> = vec![];

    let peers = peer_map.lock().await;
    for user in peers.iter() {
        users.push(
            definitions::UserOverview { 
                public_key: user.1.public_key.clone(),
                address: user.1.address.clone(),
                username: user.1.username.clone(),
                roles: user.1.roles.clone(),
                status: user.1.status.clone()
            }
        )
    }
    drop(peers);

    let last_messages = helper::retry(
        || database::get_messages(0..256, server_info.default_channel.clone(), database.clone()),
        3,
    ).await.expect("unable to get recent messages after 3 retries");

    let server_status_message = definitions::TextNetworkMessage::ServerStatus(definitions::ServerStatus {
        name: server_info.name.clone(),
        users: users,
        message_channels: server_info.message_channels.clone(),
        default_channel: server_info.default_channel.clone(),
        voice_channels: server_info.voice_channels.clone(),
        messages: last_messages,
    });

    let _ = outgoing.send(Message::Text(serde_json::to_string(&server_status_message).expect("unable to serde").into())).await;
    
    let broadcast_incoming = incoming.try_for_each(|msg| async {
        println!("Received a message from {}: {}", addr, msg.to_text().unwrap());
        match msg {
            Message::Text(text_network_msg) => {
                let msg: definitions::TextNetworkMessage = match serde_json::from_str(&text_network_msg) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Invalid message: {e} from {addr}");
                        return Ok(());
                    }
                };

                match msg {
                    definitions::TextNetworkMessage::ClientText(text_msg) => {
                        // We should probably check if they're being legit

                        let is_signed_properly = verify_signature(
                            &text_msg.plaintext,
                            &text_msg.signed_message,
                            &public_key
                        ).await;

                        if !is_signed_properly {
                            eprintln!("Invalid signature from {addr}");
                            return Ok(());
                        }

                        let time_now = SystemTime::now();

                        let since_the_epoch = time_now.duration_since(UNIX_EPOCH).unwrap();
                        let since_epoch_seconds = since_the_epoch.as_secs() as i64;

                        let broadcast_message = definitions::TextNetworkMessage::ServerText(definitions::ServerTextMessage {
                            plaintext: text_msg.plaintext.trim().to_string(),
                            sender: public_key,
                            timestamp: since_epoch_seconds,
                            channel_id: text_msg.channel.id
                        });

                        // Add message to database
                        if let definitions::TextNetworkMessage::ServerText(msg) = &broadcast_message {
                            if let Err(e) = database::add_message(msg, text_msg.channel, database.clone()).await {
                                eprintln!("add_message failed: {e}");
                            }
                        }


                        let peers = peer_map.lock().await;

                        let self_ping = true;

                        let broadcast_recipients =
                            peers.iter().filter(|(peer_addr, _)| self_ping || *peer_addr != &addr).map(|(_, ws_sink)| ws_sink);

                        for recp in broadcast_recipients {
                            println!("Sending to {addr}");
                            recp.tx.unbounded_send(Message::Text(serde_json::to_string(&broadcast_message).expect("unable to serde").into())).unwrap();
                        }
                    }
                    definitions::TextNetworkMessage::ChangeUserStatus(new_status) => {
                        let mut peers = peer_map.lock().await;

                        let self_peer = peers.get_mut(&addr);

                        if let Some(peer) = self_peer {
                            peer.status = new_status.status;
                        }
                    }
                    definitions::TextNetworkMessage::ClientRequestMessages(request) => {
                        let requested_messages = helper::retry(
                            || database::get_messages(request.range.clone(), request.channel.clone(), database.clone()),
                            3,
                        ).await.expect("unable to get recent messages after 3 retries");

                        let response = definitions::TextNetworkMessage::ServerRequestMessages(
                            definitions::ServerRequestMessages {
                                messages: requested_messages
                            }
                        );

                        tx.unbounded_send(Message::Text(serde_json::to_string(&response).expect("unable to serde").into())).unwrap();
                    }
                    _ => { } // Server messages are ignored
                }
            }
            Message::Binary(binary_msg) => {
                // TODO!
            }
            Message::Close(_) => {
                peer_map.lock().await.remove(&addr);
            }
            _ => { } // idk bro
        }

        Ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().await.remove(&addr);
}
