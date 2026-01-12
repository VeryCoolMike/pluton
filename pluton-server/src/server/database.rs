use ed25519_dalek::VerifyingKey;
use pluton_core::networking::definitions::{self, ServerTextMessage};
use std::{ops::Range, sync::Arc};
use libsql::{Database, params};

pub async fn get_messages(range: Range<u64>, database: Arc<Database>) -> anyhow::Result<Vec<ServerTextMessage>> {
    let conn = database.connect().unwrap();

    let mut query_messages = conn.query("
        SELECT plaintext, sender, timestamp, id
        FROM messages
        ORDER BY timestamp DESC, id DESC
        LIMIT ? OFFSET ?;",
        params![range.end - range.start, range.start]
    ).await?;

    let mut messages: Vec<ServerTextMessage> = vec![];

    while let Some(row) = query_messages.next().await? {
        let sender_bytes: Vec<u8> = row.get(1)?;
        let sender_array: [u8; 32] = sender_bytes
            .try_into()
            .map_err(|v: Vec<u8>| {
                anyhow::anyhow!("invalid verifying key length: {}", v.len())
            })?;

        messages.push(
            ServerTextMessage { 
                plaintext: row.get::<String>(0)?,
                sender: VerifyingKey::from_bytes(&sender_array)?,
                timestamp: row.get::<i64>(2)?
            }
        );
    }

    Ok(messages)
}

pub async fn add_message(message: &ServerTextMessage, database: Arc<Database>) -> anyhow::Result<()> {
    let conn = database.connect().unwrap();

    conn.execute("
        INSERT INTO messages (sender, plaintext, timestamp)
        VALUES (?, ?, ?);
        ", params![message.sender.as_bytes(), message.plaintext.clone(), message.timestamp]
    ).await?;

    Ok(())
}