use ed25519_dalek::VerifyingKey;
use pluton_core::{account_management, helper, networking::definitions::{self, home, ServerTextMessage, UserOverview, UserStatus}};
use pluton_core::helper::logging::{pluton_log, Importance};
use std::{ops::Range, sync::Arc};
use libsql::{Database, params};
use mime_guess;
use crate::server::{database, helper::{PeerInfo, PeerMap}};

pub async fn get_messages(range: Range<u64>, channel: definitions::Channel, database: Arc<Database>) -> anyhow::Result<Vec<ServerTextMessage>> {
    let conn = database.connect()?;

    if range.end < range.start {
        return Ok(vec![]);
    }

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
        let mut query_attachments = conn.query("
                SELECT ma.file_id, f.file_name, f.file_size
                FROM message_attachments ma
                JOIN files f ON f.id = ma.file_id
                WHERE ma.message_id = ?
                ORDER BY ma.position;", params![
                     row.get::<u64>(3)?
            ]
        ).await?;

        let mut file_vec: Vec<definitions::FileDescriptor> = vec![];

        while let Some(attachment_row) = query_attachments.next().await? {
            let constructued_descriptor = definitions::FileDescriptor {
                id: attachment_row.get(0)?,
                file_name: attachment_row.get(1)?,
                file_size: attachment_row.get(2)?
            };

            file_vec.push(constructued_descriptor);
        }

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
                attachments: file_vec,
                channel_id: channel.id
            }
        );
    }

    messages.reverse();

    Ok(messages)
}

pub async fn add_message(message: &ServerTextMessage, channel: definitions::Channel, database: Arc<Database>) -> anyhow::Result<()> {
    let conn = database.connect()?;

    let tx = conn.transaction().await?;

    tx.execute("
        INSERT INTO messages (channel_id, sender, plaintext, timestamp)
        VALUES (?, ?, ?, ?);", params![
            channel.id,
            message.sender.as_bytes(),
            message.plaintext.clone(),
            message.timestamp
        ]
    ).await?;

    // Need to also add to message_attachments so theres a link between message and file
    
    let mut row = tx.query("SELECT last_insert_rowid();", params![]).await?;
    let id: i64 = row.next().await?.unwrap().get(0)?;

    for (i, attachment) in message.attachments.iter().enumerate() {
        tx.execute("
            INSERT INTO message_attachments (message_id, file_id, position)
            VALUES (?, ?, ?);", params![
                id,
                attachment.id,
                i as u64
            ]
        ).await?;
    }

    tx.commit().await?;

    Ok(())
}

// TODO: contact home server to get rest of info
pub async fn add_user(info: &PeerInfo, database: Arc<Database>) -> anyhow::Result<()> {
    let conn = database.connect()?;

    conn.execute("
        INSERT INTO users (public_key, address)
        VALUES (?, ?);", params![info.public_key.as_bytes(), info.address.clone()]
    ).await?;

    let default_role = match get_default_role(database.clone()).await {
        Ok(m) => m,
        Err(e) => { return Err(anyhow::anyhow!(e)) }
    };

    let user_id = match user_exists(info, database.clone()).await {
        Ok(m) => {
            match m {
                Some(m) => m,
                None => return Err(anyhow::anyhow!("User does not exist after being made"))
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

// TODO! needs to contact the home server to get more info
pub async fn get_users(peer_map: PeerMap, database: Arc<Database>) -> anyhow::Result<Vec<UserOverview>> {
    let conn = database.connect()?;

    let mut query_messages = conn.query("
        SELECT public_key, address
        FROM users;", params![]
    ).await?;

    let mut users: Vec<UserOverview> = vec![];

    while let Some(row) = query_messages.next().await? {
        let verifying_key_bytes: Vec<u8> = row.get(0)?;
        let home_address: String = row.get(1)?;

        let verifying_key = helper::general::vec_to_verifying_key(verifying_key_bytes)?;

        // BAD CODE! REFACTOR WHEN YOU CAN

        // REQUEST_HOME_INFO DOESNT WORK FOR SOME REASON, RETURNS ERROR FOR NO REASON
        // BUT ITS 1:17 AM SO IM GOING TO SLEEP
        match account_management::request_home_info(
            home_address.clone(),
            verifying_key
        ).await {
            Ok(user_profile) => {
                let peers = peer_map.lock().await;
                let found = peers.values().find(|p| p.public_key == verifying_key);

                let status: UserStatus = if let Some(peer) = found {
                    peer.status.clone()
                } else {
                    UserStatus::Offline
                };

                drop(peers);
                
                let user_overview = definitions::UserOverview {
                    public_key: verifying_key,
                    address: home_address,
                    username: user_profile.username,
                    roles: get_roles(verifying_key, database.clone()).await?,
                    status
                };

                users.push(user_overview);
            }
            Err(e) => {
                pluton_log(&format!("REQUESTING HOME INFO FAILED: {e}"), Importance::Warn);
                let peers = peer_map.lock().await;
                let found = peers.values().find(|p| p.public_key == verifying_key);

                let status: UserStatus = if let Some(peer) = found {
                    peer.status.clone()
                } else {
                    UserStatus::Offline
                };

                drop(peers);
                
                let user_overview = definitions::UserOverview {
                    public_key: verifying_key,
                    address: String::new(),
                    username: String::from("Anonymous"),
                    roles: get_roles(verifying_key, database.clone()).await?,
                    status
                };

                users.push(user_overview);
            }
        }
    }

    Ok(users) 
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

pub async fn user_exists(info: &PeerInfo, database: Arc<Database>) -> anyhow::Result<Option<i64>> {
    let conn = database.connect()?;

    let mut query_user = conn.query("
        SELECT id
        FROM users
        WHERE public_key = (?)
        LIMIT 1;", params![info.public_key.as_bytes()]
    ).await?;

    if let Some(user) = query_user.next().await? {
        Ok(Some(user.get(0)?))
    } else {
        Ok(None)
    }
}

pub async fn check_role_permission(info: &PeerInfo, database: Arc<Database>, permision: definitions::RolePermissions) -> anyhow::Result<bool> {
    let conn = database.connect()?;

    let user_id = match user_exists(info, database.clone()).await {
        Ok(m) => {
            match m {
                Some(m) => m,
                None => return Err(anyhow::anyhow!("User does not exist"))
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

pub async fn get_roles(verifying_key: VerifyingKey, database: Arc<Database>) -> anyhow::Result<Vec<u8>> {
    let conn = database.connect()?;

    let mut query_roles = conn.query("
        SELECT id FROM roles
        JOIN user_roles ON roles.id = user_roles.role_id
        WHERE user_roles.user_id = (
            SELECT id FROM users
            WHERE public_key = (?)
        );
    ", params![verifying_key.as_bytes()]
    ).await?;

    let mut roles: Vec<u8> = vec![];

    while let Some(role_row) = query_roles.next().await? {
        let role_id: u64 = role_row.get(0)?;

        roles.push(role_id as u8);
    }

    Ok(roles)
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

pub async fn add_file(uploader: &VerifyingKey, file_name: &str, file_data: &[u8], timestamp: i64, database: Arc<Database>) -> anyhow::Result<i64> {
    let conn = database.connect()?;

    conn.execute("
        INSERT INTO files (uploader, file_name, file_size, timestamp, file_data)
        VALUES (?, ?, ?, ?, ?);", params![
            uploader.as_bytes(),
            file_name,
            file_data.len() as u64,
            timestamp,
            file_data,
        ]
    ).await?;

    let mut row = conn.query("SELECT last_insert_rowid();", params![]).await?;
    let id: i64 = row.next().await?.unwrap().get(0)?;

    Ok(id)
}

// Returns file data, file name, MIME type
pub async fn fetch_file(file_id: u64, database: Arc<Database>) -> anyhow::Result<(Vec<u8>, String, String)> {
    let conn = database.connect()?;

    let mut query_file = conn.query("
        SELECT file_data, file_name
        FROM files
        WHERE id = ?;", params![file_id]
    ).await?;
    
    if let Some(file) = query_file.next().await? {
        let file_name = file.get(1)?;

        let mime_type = mime_guess::from_path(&file_name)
            .first_or_octet_stream()
            .to_string();

        Ok((file.get(0)?, file_name, mime_type))        
    } else {
        Err(anyhow::anyhow!("Unable to find file from fetch_file"))
    }
}


// Returns file name, MIME type, file size
pub async fn fetch_file_meta(file_id: u64, database: Arc<Database>) -> anyhow::Result<(String, String, u64)> {
    let conn = database.connect()?;

    let mut query_file = conn.query("
        SELECT file_name, file_size
        FROM files
        WHERE id = ?;", params![file_id]
    ).await?;
    
    if let Some(file) = query_file.next().await? {
        let file_name = file.get(0)?;

        let mime_type = mime_guess::from_path(&file_name)
            .first_or_octet_stream()
            .to_string();

        Ok((file_name, mime_type, file.get(1)?))
    } else {
        Err(anyhow::anyhow!("Unable to find file from fetch_file_meta"))
    }
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
