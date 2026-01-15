use serde::{Serialize, Deserialize};
use serde_with::{DisplayFromStr, TimestampSeconds, serde_as};
use ed25519_dalek::{Signature, VerifyingKey};
use std::ops::Range;

pub const VERSION: u32 = 1;

// Authentication

/*
Autentication in the Pluton protocol works on the basic idea of the challenge response system.
A client first sends a "handshake start" along with metadata and other information to the server,
the server responds to the client with a challenge.
The server sends the client a randomly generated string for the client to sign to prove they are who they say they are.
This challenge has a default duration of 120 seconds, where if the client does not respond within 120 seconds, they are disconnected.
The client signs the randomly generated string and then sends it back to the server.
The server verifies it and either lets the client in or doesn't.
NOT IMPLEMENTED:
After the server verifies the client, it sends them a session token which allows the client to join back after a disconnection.
*/

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ClientAuthMessage {
    InitiateHandshake(ClientHandshakeStart),
    ResumeSession(ClientSessionResume),
    CompleteHandshake(ClientHandshakeFinal)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientHandshakeStart {
    pub username: String,
    pub public_key: VerifyingKey,
    pub address: String,
    pub version: u32
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientSessionResume {
    pub session_token: String,
    pub version: u32
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerChallenge {
    pub nonce: String,
    pub version: u32
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct ClientHandshakeFinal {
    pub signed_message: Signature
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerResponse {
    pub session_token: String,
    pub status_code: Result<HandshakeStatus, HandshakeError>
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HandshakeStatus {
    Complete,
    Rejected
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HandshakeError {
    ServerError,
    ClientError,
    SendFailed,
    InvalidSignature,
    BadCryptography,
    SerializationError
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
    ServerRequestMessages(ServerRequestMessages)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientTextMessage {
    pub plaintext: String,
    pub signed_message: Signature,
    pub id: u32,
    pub channel: Channel
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerTextMessage {
    pub plaintext: String,
    pub sender: VerifyingKey,
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