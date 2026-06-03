use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use axum::{
    Router, extract::{Multipart, State, Path, DefaultBodyLimit}, http::StatusCode, response::IntoResponse, routing::{post, get}, Json
};
use pluton_core::helper::logging::{pluton_log, Importance};
use libsql::{params, Database};
use pluton_core::networking::definitions;
use reqwest::header;
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
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .route("/download_file/{file_id}", get(download_file))
        .route("/download_file_meta/{file_id}", get(download_file_meta))
        .with_state(shared_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 6766));
    pluton_log(&format!("Pluton file server listening on {}", addr), Importance::Info);

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

    pluton_log(&format!("Uploading file: {file_name}"), Importance::Info);

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

    pluton_log(&format!("Finished uploading file: {file_name}"), Importance::Info);
    Ok(Json(file_id))
}

async fn download_file(
    Path(file_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    let file_id = file_id.parse::<u64>().map_err(|_| StatusCode::BAD_REQUEST)?;

    let (file_data, file_name, mime_type) = database::fetch_file(file_id, state.db.clone()).await.map_err(|_| StatusCode::NOT_FOUND)?;

    let ascii_fallback: String = file_name
        .chars()
        .map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' { c } else { '_' })
        .collect();

    let disposition = format!(
        "attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{}",
        urlencoding::encode(&file_name),
    );

    let headers = [
        (header::CONTENT_TYPE, mime_type),
        (header::CONTENT_DISPOSITION, disposition),
    ];

    Ok((headers, file_data))
}

async fn download_file_meta(
    Path(file_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<definitions::FileDescriptor>, StatusCode> {
    let file_id = file_id.parse::<u64>().map_err(|_| StatusCode::BAD_REQUEST)?;

    let (file_name, _, file_size) = database::fetch_file_meta(file_id, state.db.clone()).await.map_err(|_| StatusCode::NOT_FOUND)?;

    let descriptor = definitions::FileDescriptor {
        id: file_id,
        file_name: file_name,
        file_size: file_size
    };

    Ok(axum::Json(descriptor))
}
