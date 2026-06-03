use std::{
    collections::HashMap, sync::Arc, time::{SystemTime, UNIX_EPOCH}
};

use libsql::{Builder, params};

use pluton_core::{cryptography::{sign_message, verify_signature}, networking::definitions::{self, UserOverview}};
use pluton_core::helper::logging::{pluton_log, Importance};
mod database;
mod helper;
mod connections;
mod moderation;
mod home;
mod http;

use tokio::{net::{TcpListener, TcpStream}, sync::{broadcast, Mutex}};

pub async fn start_server() -> anyhow::Result<()> {
    let db = Arc::new(
        Builder::new_local("server_data.db").build().await?
    );
    let conn = db.connect()?;

    conn.query("PRAGMA journal_mode = WAL;", params![]).await?;
    conn.execute("PRAGMA synchronous = NORMAL;", params![]).await?;
    conn.execute("PRAGMA foreign_keys = ON;", params![]).await?;

    let mut rows = conn.query(
        "
            SELECT server_ip, server_port, server_name FROM server
        "
    , params![]).await?;

    let mut addr: String = String::new();
    let mut server_name: String = String::new();

    if let Some(row) = rows.next().await? {
        let ip: String = row.get(0)?;
        let port: String = row.get(1)?;
        server_name = row.get(2)?;
        addr = format!("{}:{}", ip, port);
        pluton_log(&format!("Attempting to connect to {}", ip), Importance::Info);
    }

    let state = helper::PeerMap::new(Mutex::new(HashMap::new()));

    let server_info = Arc::new(helper::ServerInfo{
        name: server_name,
        message_channels: database::get_message_channels(db.clone()).await.unwrap(),
        default_channel: database::get_default_channel(db.clone()).await.unwrap(),
        voice_channels: Vec::new()
    });

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    pluton_log(&format!("Listening on: {}", addr), Importance::Info);

    let http_handle = tokio::spawn(http::start_http_server(db.clone(), state.clone()));

    // Let's spawn the handling of each connection in a separate task.
    let tcp_handle = tokio::spawn(async move {
        while let Ok((stream, addr)) = listener.accept().await {
            tokio::spawn(connections::handle_connection(
                state.clone(),
                stream,
                addr,
                db.clone(),
                server_info.clone()
            ));
        }
    });

    tokio::try_join!(http_handle, tcp_handle)?;

    Ok(())
}
