use ed25519_dalek::SigningKey;
use pluton_core::networking::definitions;

/// Events the network task sends to the TUI.
pub enum NetEvent {
    Connected,
    HandshakeOk(String),
    SigningKey(SigningKey),
    Error(String),
    Incoming(definitions::TextNetworkMessage),
    Disconnected,
}
