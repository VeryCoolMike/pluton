use serde::{Serialize, Deserialize};
use ed25519_dalek::{Signature, VerifyingKey, SigningKey};
use std::{ops::Range, collections::HashMap};

pub const VERSION: u32 = 1;

mod authentication;
pub mod home;

pub use authentication::*;

// Client Only
#[derive(Clone, Debug)]
pub struct Peer {
    pub username: String,
    pub address: String,
    pub roles: Vec<u8>,
    pub status: UserStatus
}

pub struct ClientState {
    pub peers: HashMap<VerifyingKey, Peer>,
    pub current_message_id: u32,
    pub signing_key: SigningKey,
    pub current_channel: Channel,
    pub current_messages: Vec<ServerTextMessage>,
    pub message_channels: Vec<Channel>,
    pub voice_channels: Vec<Channel>
}

// Text Messages
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum TextNetworkMessage {
    ClientText(ClientTextMessage),
    ServerText(ServerTextMessage),
    UserStatusChange(UserStatusChange),
    ChangeUserStatus(ChangeUserStatus),
    ServerStatus(ServerStatus),
    ClientRequestMessages(ClientRequestMessages),
    ServerRequestMessages(ServerRequestMessages),
    UserJoin(UserJoin),

    // Moderation
    ClientKickRequest(ClientKickRequest),
    ServerKickRequest(ServerKickRequest)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientTextMessage {
    pub plaintext: String,
    pub signed_message: Signature,
    pub id: u32,
    pub channel: Channel
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerTextMessage {
    pub plaintext: String,
    pub sender: VerifyingKey,
    pub channel_id: u64,
    pub timestamp: i64 // Time from UNIX EPOCH
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientRequestMessages {
    pub range: Range<u64>,
    pub channel: Channel
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerRequestMessages {
    pub messages: Vec<ServerTextMessage>
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum UserStatus {
    Online,
    DoNotDisturb,
    Sleep,
    Offline
}

// Server -> Client
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserJoin {
    pub public_key: VerifyingKey,
    pub username: String,
    pub address: String
}

// Server -> Client
#[derive(Serialize, Deserialize, Debug)]
pub struct UserStatusChange {
    pub public_key: VerifyingKey,
    pub status: UserStatus
}

// Client -> Server
#[derive(Serialize, Deserialize, Debug)]
pub struct ChangeUserStatus {
    pub status: UserStatus
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserOverview {
    pub public_key: VerifyingKey,
    pub address: String,
    pub username: String,
    pub roles: Vec<u8>,
    pub status: UserStatus
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerStatus {
    pub name: String,
    pub users: Vec<UserOverview>,
    pub message_channels: Vec<Channel>,
    pub default_channel: Channel,
    pub voice_channels: Vec<Channel>,
    pub messages: Vec<ServerTextMessage> // Last ??? messages,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Channel {
    pub id: u64,
    pub name: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RolePermissions {
    Kick 
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientKickRequest {
    pub recipient: VerifyingKey,
    pub reason: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerKickRequest {
    pub recipient: VerifyingKey,
    pub reason: String,
    pub sender: String
}
