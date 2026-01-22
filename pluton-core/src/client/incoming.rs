use chrono::{Local, Utc, TimeZone, DateTime};
use crate::networking::definitions;
use tokio::sync::Mutex;
use std::sync::Arc;

pub async fn receive_server_text(
    incoming: definitions::ServerTextMessage,
    client_state: Arc<Mutex<definitions::ClientState>>
) -> anyhow::Result<(DateTime<Local>, String)> {
    let client_lock = client_state.lock().await;
    let peers = client_lock.peers.clone();
    let current_channel = client_lock.current_channel.clone();
    drop(client_lock);

    if incoming.channel_id != current_channel.id {
        return Err(anyhow::anyhow!("wrong channel"));
    }

    let sender_username = match peers.get(&incoming.sender) {
        Some(peer) => {
            peer.username.clone()
        }
        None => String::from("Error")
    };

    let datetime = Utc.timestamp_opt(incoming.timestamp, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid timestamp"))?
        .with_timezone(&Local);

    Ok((datetime, sender_username))
}

pub async fn receive_server_status(
    incoming: definitions::ServerStatus,
    client_state: Arc<Mutex<definitions::ClientState>>
) -> anyhow::Result<()> {
    let mut client_lock = client_state.lock().await;
    // Let's get some peers
    for user in incoming.users {
        client_lock.peers.insert(
            user.public_key,
            definitions::Peer {
                username: user.username,
                address: user.address,
                roles: user.roles,
                status: user.status
            }
        );
    }

    client_lock.current_channel = incoming.default_channel;
    client_lock.message_channels = incoming.message_channels;
    client_lock.voice_channels = incoming.voice_channels;
    client_lock.current_messages = incoming.messages.clone();

    drop(client_lock);

    Ok(())
}