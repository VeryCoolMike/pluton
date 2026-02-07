use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct UserProfile {
    pub username: String,
    pub biography: String,
}

#[derive(Serialize, Deserialize)]
pub enum ChangeAction {
    Username(String),
    Biography(String)
}

#[derive(Serialize, Deserialize)]
pub struct ChangeRequestPayload {
    pub timestamp: i64, // time from epoch
    pub action: ChangeAction,
}

#[derive(Serialize, Deserialize)]
pub struct AccountCreation {
    profile: UserProfile,
    timestamp: i64
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SignedRequest {
    pub public_key: String, // base64
    pub payload: String,
    pub signature: String // base64
}
