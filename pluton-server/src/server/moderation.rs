use anyhow::anyhow;
use pluton_core::networking::definitions;
use std::sync::Arc;
use libsql::{Database, params};
use tokio_tungstenite::tungstenite::protocol::Message;

use crate::server::{helper, database};

pub async fn kick_user(request: definitions::ClientKickRequest, clients: helper::PeerMap, database: Arc<Database>) -> Result<(), anyhow::Error> {
    // First we need to remove the client
    let recipient_addr = {
        let client_lock = clients.lock().await;
        client_lock
            .iter()
            .find(|(_, k)| k.public_key == request.recipient)
            .map(|(k, _)| *k)
    };

    let Some(recipient_addr) = recipient_addr else { 
        return Err(anyhow::anyhow!("Recipient not found"));
    };


    let mut client_lock = clients.lock().await;
    
    let peer = if let Some(peer) = client_lock.get(&recipient_addr) {
        let _ = peer.tx.unbounded_send(Message::Close(None));
        peer
    } else {
        return Err(anyhow::anyhow!("Recipient not found"));
    };

    database::remove_user(peer, database).await?;

    client_lock.remove(&recipient_addr);

    Ok(())
}
