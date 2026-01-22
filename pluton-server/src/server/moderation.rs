use pluton_core::networking::definitions;
use std::sync::Arc;
use libsql::{Database, params};
use tokio_tungstenite::tungstenite::protocol::Message;

use crate::server::{helper, database};

pub async fn kick_user(request: definitions::ClientKickRequest, clients: helper::PeerMap, database: Arc<Database>) {
    // First we need to remove the client
    let recipient = {
        let client_lock = clients.lock().await;
        client_lock
            .iter()
            .find(|(_, k)| k.public_key == request.recipient)
            .map(|(k, _)| *k)
    };

    let Some(recipient) = recipient else { return };

    let mut client_lock = clients.lock().await;
    
    if let Some(peer) = client_lock.get(&recipient) {
        let _ = peer.tx.unbounded_send(Message::Close(None));
    }

    client_lock.remove(&recipient);

    database.remove_user(request.recipient);
}
