use ed25519_dalek::VerifyingKey;
use pluton_core::{helper, networking::definitions::{self, ServerTextMessage}};
use std::{ops::Range, sync::Arc};
use libsql::{Database, params};
use crate::server::helper::PeerInfo;

pub async fn get_messages(range: Range<u64>, channel: definitions::Channel, database: Arc<Database>) -> anyhow::Result<Vec<ServerTextMessage>> {
    let conn = database.connect()?;

    let mut query_messages = conn.query("
        SELECT plaintext, sender, timestamp, id
        FROM messages
        WHERE channel_id = ?
        ORDER BY timestamp DESC, id DESC
        LIMIT ? OFFSET ?;", params![
            channel.id,
            range.end - range.start,
            range.start
        ]
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
                timestamp: row.get::<i64>(2)?,
                channel_id: channel.id
            }
        );
    }

    messages.reverse();

    Ok(messages)
}

pub async fn add_message(message: &ServerTextMessage, channel: definitions::Channel, database: Arc<Database>) -> anyhow::Result<()> {
    let conn = database.connect()?;

    conn.execute("
        INSERT INTO messages (channel_id, sender, plaintext, timestamp)
        VALUES (?, ?, ?, ?);", params![
            channel.id,
            message.sender.as_bytes(),
            message.plaintext.clone(),
            message.timestamp
        ]
    ).await?;

    Ok(())
}

pub async fn add_user(info: &PeerInfo, database: Arc<Database>) -> anyhow::Result<()> {
    let conn = database.connect()?;

    conn.execute("
        INSERT INTO users (public_key)
        VALUES (?);", params![info.public_key.as_bytes()]
    ).await?;

    let default_role = match get_default_role(database.clone()).await {
        Ok(m) => m,
        Err(e) => { return Err(anyhow::anyhow!(e)) }
    };

    let user_id = match user_exists(info, database.clone()).await {
        Ok(m) => {
            match m {
                Some(v) => v,
                None => { return Err(anyhow::anyhow!("User somehow does not exist after successful creation")) }
            }
        },
        Err(e) => { return Err(anyhow::anyhow!(e)) }
    };

    conn.execute("
        INSERT INTO user_roles (user_id, role_id)
        VALUES (?, ?);", params![user_id, default_role]
    ).await?;

    Ok(())
} 

pub async fn remove_user(info: &PeerInfo, database: Arc<Database>) -> anyhow::Result<()> {
    let conn = database.connect()?;

    conn.execute("
        DELETE
        FROM users
        WHERE public_key = (?);", params![info.public_key.as_bytes()]
    ).await?;

    Ok(())
}

pub async fn user_exists(info: &PeerInfo, database: Arc<Database>) -> anyhow::Result<Option<bool>> {
    let conn = database.connect()?;

    let mut query_user = conn.query("
        SELECT id
        FROM users
        WHERE public_key = (?)
        LIMIT 1;", params![info.public_key.as_bytes()]
    ).await?;

    if let Some(user) = query_user.next().await? {
        Ok(
            Some(user.get(0)?)
        )
    } else {
        Ok(None)
    }
}

pub async fn check_role_permission(info: &PeerInfo, database: Arc<Database>, permision: definitions::RolePermissions) -> anyhow::Result<bool> {
    let conn = database.connect()?;

    let user_id = match user_exists(info, database.clone()).await {
        Ok(m) => {
            match m {
                Some(v) => v,
                None => { return Err(anyhow::anyhow!("User does not exist")) }
            }
        },
        Err(e) => { return Err(anyhow::anyhow!(e)) }
    };

    let mut query_user_roles = conn.query("
        SELECT user_roles.user_id, roles.can_kick
        FROM user_roles
        INNER JOIN roles ON user_roles.role_id=roles.id
        WHERE user_roles.user_id = (?);", params![user_id]
    ).await?;

    while let Some(row) = query_user_roles.next().await? {
        let can_kick = row.get::<i64>(1)?;

        match permision {
            definitions::RolePermissions::Kick => {
                if can_kick == 1 {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

pub async fn get_default_role(database: Arc<Database>) -> anyhow::Result<u64> {
    let conn = database.connect()?;

    let mut query_default_role = conn.query("
        SELECT id
        FROM roles
        WHERE id = (SELECT default_role FROM server LIMIT 1);", ()
    ).await?;

    if let Some(default_role) = query_default_role.next().await? {
        let id: u64 = default_role.get(0)?;
        
        return Ok(id);
    }

    Err(anyhow::anyhow!("default channel not found"))
}

pub async fn get_default_channel(database: Arc<Database>) -> anyhow::Result<definitions::Channel> {
    let conn = database.connect()?;

    let mut query_default_channel = conn.query("
        SELECT id, name
        FROM channels
        WHERE id = (SELECT default_channel FROM server LIMIT 1);", ()
    ).await?;

    if let Some(default_channel) = query_default_channel.next().await? {
        let id: u64 = default_channel.get(0)?;
        let name: String = default_channel.get(1)?;
        
        let channel = definitions::Channel {
            id,
            name
        };

        return Ok(channel);
    }

    Err(anyhow::anyhow!("default channel not found"))
}

pub async fn get_message_channels(database: Arc<Database>) -> anyhow::Result<Vec<definitions::Channel>> {
    let conn = database.connect()?;

    let mut query_message_channels = conn.query("
        SELECT id, name
        FROM channels
        WHERE type = 'text';", ()
    ).await?;

    let mut message_channels: Vec<definitions::Channel> = vec![];

    while let Some(channel) = query_message_channels.next().await? {
        let id: u64 = channel.get(0)?;
        let name: String = channel.get(1)?;
        
        message_channels.push(definitions::Channel {
            id,
            name
        });

    }

    Ok(message_channels)
}
