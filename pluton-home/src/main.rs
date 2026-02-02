use std::net::SocketAddr;
use std::sync::Arc;
use axum::{
    Router, extract::{Json, Path, State}, http::{Error, StatusCode}, response::IntoResponse, routing::{get, post}
};
use libsql::{Builder, params, Database};
use serde::{Serialize, Deserialize};
use pluton_core::helper;

#[derive(Serialize, Deserialize)]
struct UserProfile {
    username: String,
    biography: String,
}

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
) -> Result<Json<UserProfile>, StatusCode> {
    let conn = state.clone().db.connect().expect("Unable to connect to database");


    // Have to check find the user
    let mut user_row = conn.query("
        SELECT username, biography
        FROM users
        WHERE public_key = (?)
    ", params![helper::base64::from_base64(public_key)])
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;


    if let Some(user) = user_row.next().await.map_err(|_| StatusCode::NOT_FOUND)? {
        let profile = UserProfile {
            username: user.get(0).map_err(|_| StatusCode::NOT_FOUND)?,
            biography: user.get(1).map_err(|_| StatusCode::NOT_FOUND)?
        };

        return Ok(axum::Json(profile));
    }

    Err(StatusCode::IM_A_TEAPOT)
}

async fn update_profile(
) {

}
