use std::net::SocketAddr;
use std::sync::Arc;
use axum::{
    Router, extract::{Json, Path, State}, http::{Error, StatusCode}, response::IntoResponse, routing::{get, post}
};
use libsql::{Builder, params, Database};
use pluton_core::{helper, networking::definitions};
use ed25519_dalek::{VerifyingKey, Signature};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct AppState {
    db: Arc<Database>
}

#[tokio::main]
async fn main() {
    let db = Builder::new_local("home_data.db").build().await.expect("Cannot establish database");
    let conn = db.connect().expect("Cannot connect to database");

    conn.execute("
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY,
            public_key BLOB NOT NULL,
            username TEXT,
            biography TEXT,
            signature TEXT
        );
    ", ()
    ).await.expect("Unable to create main table");

    conn.query("PRAGMA journal_mode = WAL;", params![]).await.expect("Unable to enable WAL");
    conn.execute("PRAGMA synchronous = NORMAL;", params![]).await.expect("Unable to change synchronous settings");
    conn.execute("PRAGMA foreign_keys = ON;", params![]).await.expect("Unable to enable foreign keys");

    let mut shared_state = Arc::new(AppState {
        db: Arc::new(db)
    });


    let app = Router::new()
        .route("/profile/{public_key}", get(get_profile))
        .route("/profile", post(update_profile))
        .with_state(shared_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 6768));
    println!("Home Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_profile(
    Path(public_key): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<definitions::home::UserProfile>, StatusCode> {
    let conn = state.clone().db.connect().expect("Unable to connect to database");


    // Have to check find the user
    let mut user_row = conn.query("
        SELECT username, biography
        FROM users
        WHERE public_key = (?);
    ", params![helper::base64::from_base64(public_key)])
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;


    if let Some(user) = user_row.next().await.map_err(|_| StatusCode::NOT_FOUND)? {
        let profile = definitions::home::UserProfile {
            username: user.get(0).map_err(|_| StatusCode::NOT_FOUND)?,
            biography: user.get(1).map_err(|_| StatusCode::NOT_FOUND)?
        };

        return Ok(axum::Json(profile));
    }

    Err(StatusCode::IM_A_TEAPOT)
}

async fn update_profile(
    State(state): State<Arc<AppState>>,
    Json(request): Json<definitions::home::SignedChangeRequest>
) -> Result<StatusCode, StatusCode> {
    let conn = state.clone().db.connect().expect("Unable to connect to database");

    // We need to check if the signature matches the payload
    
    let key_bytes: [u8; 32] = helper::base64::from_base64(request.public_key.clone())
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let public_key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let signature_bytes: [u8; 64] = helper::base64::from_base64(request.signature)
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let signature = Signature::from_bytes(&signature_bytes);
    
    let verified = pluton_core::cryptography::verify_signature(
        &request.payload,
        &signature,
        &public_key
    ).await;

    if !verified {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let payload: definitions::home::ChangeRequestPayload = serde_json::from_str(&request.payload)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let now = SystemTime::now();

    let since_the_epoch: i64 = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs() as i64;

    if (since_the_epoch - payload.timestamp).abs() > 60 {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut user_row = conn.query("
        SELECT id
        FROM users
        WHERE public_key = (?);
    ", params![helper::base64::from_base64(request.public_key)])
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    match payload.action {
        definitions::home::ChangeAction::Username(new_name) => {
            if let Some(user) = user_row.next().await.map_err(|_| StatusCode::NOT_FOUND)? {
                let id: i64 = user.get(0).map_err(|_| StatusCode::NOT_FOUND)?;

                conn.execute("
                    UPDATE users
                    SET username = (?)
                    WHERE id = (?);
                ", params![new_name, id])
                    .await
                    .map_err(|_| StatusCode::NOT_MODIFIED)?;
            }
        }
        definitions::home::ChangeAction::Biography(new_bio) => {
            if let Some(user) = user_row.next().await.map_err(|_| StatusCode::NOT_FOUND)? {
                let id: i64 = user.get(0).map_err(|_| StatusCode::NOT_FOUND)?;

                conn.execute("
                    UPDATE users
                    SET biography = (?)
                    WHERE id = (?);
                ", params![new_bio, id])
                    .await
                    .map_err(|_| StatusCode::NOT_MODIFIED)?;
            }
        }
    }

    Ok(StatusCode::OK)
}
