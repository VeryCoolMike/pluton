use std::net::SocketAddr;
use std::sync::Arc;
use axum::{
    Router, extract::{Json, Path, State}, http::{Error, StatusCode}, response::IntoResponse, routing::{get, post}
};
use ed25519_dalek::{ed25519::signature, Signature, VerifyingKey};
use libsql::{Builder, params, Database};
use serde::{Serialize, Deserialize};
use pluton_core::{cryptography, helper, networking::definitions::{self, home}};
use std::time::{SystemTime, UNIX_EPOCH};
use pluton_core::networking::definitions::home::UserProfile;

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
            verifying_key BLOB NOT NULL,
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
        .route("/profile/{verifying_key}", get(get_profile))
        .route("/profile", post(update_profile))
        .route("/create_profile", post(create_profile))
        .with_state(shared_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 6768));
    println!("Home Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn check_user_key(payload: Json<home::SignedRequest>) -> Result<bool, StatusCode> {
    println!("Attempting to check user key");
    let signature_vec = helper::base64::from_base64url(payload.signature.clone());
    let signature_bytes: [u8; 64] = signature_vec
        .as_slice()
        .try_into()
        .map_err(|e| {println!("{e}"); StatusCode::BAD_REQUEST})?;
    println!("Changed signature into array (correct size)");

    let signature: Signature = Signature::from_bytes(&signature_bytes);

    let verifying_key_vec = helper::base64::from_base64url(payload.verifying_key.clone());
    let verifying_key_bytes: [u8; 32] = verifying_key_vec
        .as_slice()
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    println!("Changed verifying key into array (correct size)");

    let verifying_key: VerifyingKey = VerifyingKey::from_bytes(&verifying_key_bytes)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    println!("Deserialised verifying key");

    return Ok(cryptography::verify_signature(&payload.payload, &signature, &verifying_key).await);
}

async fn check_user_full(payload: Json<home::SignedRequest>) -> Result<(), StatusCode> {
    println!("Attempting to check user full");
    if !check_user_key(payload.clone()).await? {
        return Err(StatusCode::BAD_REQUEST);
    }
    println!("Checked user key");

    #[derive(Deserialize)]
    struct TimestampCheck {
        timestamp: i64
    }

    let request: TimestampCheck = serde_json::from_str(&payload.payload)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs() as i64;

    if (current_time - request.timestamp).abs() > 60 {
        return Err(StatusCode::BAD_REQUEST);
    }
    println!("User signed timestamp checked");

    Ok(())
}

async fn get_profile(
    Path(verifying_key): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<UserProfile>, StatusCode> {
    let conn = state.clone().db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Have to check find the user
    let mut user_row = conn.query("
        SELECT username, biography
        FROM users
        WHERE verifying_key = (?)
    ", params![helper::base64::from_base64url(verifying_key)])
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    if let Some(user) = user_row.next().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let profile = UserProfile {
            username: user.get(0).map_err(|_| StatusCode::BAD_REQUEST)?,
            biography: user.get(1).map_err(|_| StatusCode::BAD_REQUEST)?
        };

        return Ok(axum::Json(profile));
    }

    Err(StatusCode::BAD_REQUEST)
}

async fn update_profile(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<home::SignedRequest>
) -> Result<StatusCode, StatusCode> {
    if let Err(e) = check_user_full(Json(payload.clone())).await {
        return Err(StatusCode::BAD_REQUEST);
    };

    let verifying_key_bytes: Vec<u8> = helper::base64::from_base64url(payload.verifying_key);

    let conn = state.clone().db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let request: home::ChangeRequestPayload = serde_json::from_str(&payload.payload)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match request.action {
        home::ChangeAction::Username(new_username) => {
            conn.execute("
                UPDATE users
                SET username = (?)
                WHERE verifying_key = (?)
            ", params![new_username, verifying_key_bytes])
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
        }
        home::ChangeAction::Biography(new_biography) => {
            conn.execute("
                UPDATE users
                SET biography = (?)
                WHERE verifying_key = (?)
            ", params![new_biography, verifying_key_bytes])
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;
        }
    }

    Ok(StatusCode::OK)
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<home::SignedRequest>
) -> Result<StatusCode, StatusCode> {
    println!("Attempting to add user");
    if let Err(e) = check_user_full(Json(payload.clone())).await {
        return Err(StatusCode::BAD_REQUEST);
    };
    println!("User has been checked");

    let verifying_key_bytes: Vec<u8> = helper::base64::from_base64url(payload.verifying_key);

    let conn = state.clone().db.connect().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let request: home::AccountCreation = serde_json::from_str(&payload.payload)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    println!("Request has been deserialised");

    conn.execute("
        INSERT INTO users (verifying_key, username, biography)
        VALUES (?, ?, ?)
    ", params![verifying_key_bytes, request.profile.username, request.profile.biography])
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    println!("User has been added");

    Ok(StatusCode::OK)
}
