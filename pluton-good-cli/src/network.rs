use ed25519_dalek::{SigningKey, VerifyingKey};
use pluton_core::networking::definitions;

/// Events the network task sends to the TUI.
pub enum NetEvent {
    Connected,
    HandshakeOk(String),
    SessionGranted(String),
    SigningKey(SigningKey),
    VerifyingKey(VerifyingKey),
    Error(String),
    Incoming(definitions::TextNetworkMessage),
    Disconnected,
}
