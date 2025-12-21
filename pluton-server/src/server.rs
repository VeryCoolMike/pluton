use std::{
    collections::HashMap, env, fmt::Binary, net::SocketAddr, sync::{Arc, Mutex}, time::{SystemTime, UNIX_EPOCH}
};

use egui::epaint::text;
use serde::{Deserialize, Serialize};
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{SinkExt, StreamExt, future::{self, join}, pin_mut, stream::TryStreamExt};
use libsql::{Builder, params};

use pluton_core::{cryptography::{sign_message, verify_signature}, networking::definitions};
use ed25519_dalek::VerifyingKey;

use tokio::{net::{TcpListener, TcpStream}, sync::broadcast};
use tokio_tungstenite::tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;

#[derive(Debug)]
struct PeerInfo {
    username: String,
    tx: Tx,
    public_key: VerifyingKey,
    permission: u8
}
type PeerMap = Arc<Mutex<HashMap<SocketAddr, PeerInfo>>>;


async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr, conn: libsql::Connection) {
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
    let info = PeerInfo {
        username: join_request.username,
        tx,
        public_key: join_request.public_key,
        permission: 0
    };
    let public_key = info.public_key.clone();

    peer_map.lock().unwrap().insert(addr, info);
    println!("{} has been accepted!", addr);
    println!("[DEBUG] {:?}", peer_map.lock().unwrap());


    let broadcast_recipients: Vec<Tx> = {
        let peers = peer_map.lock().unwrap();       
        peers.iter().filter(|(peer_addr, _)| *peer_addr != &addr).map(|(_, peer)| peer.tx.clone()).collect()
    };

    let join_alert = definitions::UserStatusChange {
        public_key: public_key,
        address: String::new(),
        status: definitions::UserStatus::Online
    };
    for tx in broadcast_recipients {
        tx.unbounded_send(Message::Text(serde_json::to_string(&join_alert).expect("unable to serde").into())).unwrap();
    }

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
                            plaintext: text_msg.plaintext,
                            sender: public_key,
                            timestamp: since_epoch_seconds
                        });


                        let peers = peer_map.lock().unwrap();

                        let self_ping = true;

                        let broadcast_recipients =
                            peers.iter().filter(|(peer_addr, _)| self_ping || *peer_addr != &addr).map(|(_, ws_sink)| ws_sink);

                        for recp in broadcast_recipients {
                            println!("Sending to {:?}", recp.public_key);
                            recp.tx.unbounded_send(Message::Text(serde_json::to_string(&broadcast_message).expect("unable to serde").into())).unwrap();
                        }
                    }
                    _ => { } // Server messages are ignored
                }
            }
            Message::Binary(binary_msg) => {
                // TODO!
            }
            Message::Close(_) => {
                peer_map.lock().unwrap().remove(&addr);
            }
            _ => { } // idk bro
        }

        Ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().unwrap().remove(&addr);
}

pub async fn start_server() -> anyhow::Result<()> {
    let server_data_path = std::path::Path::new("server_data.db");
    if !server_data_path.exists() {
        println!("A server has not been made yet, please create it by running with the flag --create_server.");
        return Ok(());
    }

    let db = Builder::new_local("server_data.db").build().await?;
    let conn = db.connect()?;

    let mut rows = conn.query(
        "
            SELECT server_ip, server_port FROM server
        "
    , params![]).await?;

    let mut addr: String = String::new();

    if let Some(row) = rows.next().await? {
        let ip: String = row.get(0)?;
        let port: String = row.get(1)?;
        addr = format!("{}:{}", ip, port);
        println!("Attempting to connect to {}", ip);
    }

    let state = PeerMap::new(Mutex::new(HashMap::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr, conn.clone()));
    }

    Ok(())
}
