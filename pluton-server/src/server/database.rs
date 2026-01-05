use ed25519_dalek::VerifyingKey;
use pluton_core::networking::definitions::{self, ServerTextMessage};
use std::ops::Range;

pub async fn get_messages(range: Range<u64>) -> Vec<ServerTextMessage> {
    let mut messages: Vec<ServerTextMessage> = vec![];
    for _ in range {
        messages.push(
            ServerTextMessage { plaintext: String::new(), sender: VerifyingKey::default(), timestamp: 0 }
        );
    }

    messages
}