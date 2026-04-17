use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use axum::{
    Router, extract::{Multipart, State}, http::StatusCode, response::IntoResponse, routing::post, Json
};
use libsql::{params, Database};
use super::{helper::PeerMap, database};

#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
    peers: PeerMap
}

pub async fn start_http_server(db: Arc<Database>, peers: PeerMap) {
    let shared_state = Arc::new(AppState {
        db,
        peers
    });

    let app = Router::new()
        .route("/upload_file", post(upload_file))
        .with_state(shared_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 6766));
    println!("Pluton file server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn upload_file(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, StatusCode> {
    let mut session_id: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "session_id" => {
                session_id = Some(field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?);
            }
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                file_data = Some(field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?.to_vec());
            }
            _ => {}
        }
    }

    let session_id = session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let file_name = file_name.ok_or(StatusCode::BAD_REQUEST)?;
    let file_data = file_data.ok_or(StatusCode::BAD_REQUEST)?;

    // Validate session ID
    let peers = state.peers.lock().await;
    let uploader = peers.values()
        .find(|peer| peer.session_id == session_id)
        .map(|peer| peer.public_key);
    drop(peers);

    let uploader = uploader.ok_or(StatusCode::UNAUTHORIZED)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs() as i64;

    let file_id = database::add_file(&uploader, &file_name, &file_data, timestamp, state.db.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(file_id))
}
