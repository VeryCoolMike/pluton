use std::net::SocketAddr;
use tokio::net::TcpStream;

use ed25519_dalek::{SigningKey, VerifyingKey};
use tokio_tungstenite::{self, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::Message;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{StreamExt, SinkExt};
use rand::{Rng, distributions::Alphanumeric};

use crate::networking::definitions::{self, HandshakeStatus};
use crate::cryptography::{sign_message, verify_signature};

pub async fn auth_handshake_client(
    outgoing: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    incoming: &mut SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    username: &str,
    public_key: VerifyingKey,
    address: String,
    signing_key: SigningKey
) -> Result<definitions::HandshakeStatus, definitions::HandshakeError> {

    let handshake_start = definitions::ClientHandshakeStart {
        username: username.to_string(),
        public_key: public_key,
        address: address,
        version: definitions::VERSION
    };
    let request_msg = Message::Text(
        serde_json::to_string(&handshake_start)
            .map_err(|_| definitions::HandshakeError::SerializationError)?
            .into()
    );
    outgoing.send(request_msg).await
        .map_err(|_| definitions::HandshakeError::SendFailed)?;

    let first_response = incoming.next().await;

    let challenge_accept: definitions::ServerChallenge = if let Some(Ok(Message::Text(text))) = first_response {
        match serde_json::from_str(&text) {
            Ok(data) => data,
            Err(_) => {
                return Err(definitions::HandshakeError::SerializationError);
            }
        }
    } else {
        return Err(definitions::HandshakeError::ServerError);
    };

    let signed_nonce = sign_message(&challenge_accept.nonce, &signing_key).await;

    let challenge_finish = definitions::ClientHandshakeFinal {
        signed_message: signed_nonce
    };
    let challenge_finish_msg = Message::Text(
        serde_json::to_string(&challenge_finish)
        .map_err(|_| definitions::HandshakeError::SerializationError)?
        .into()
    );
    outgoing.send(challenge_finish_msg).await
        .map_err(|_| definitions::HandshakeError::SendFailed)?;

    let final_response = incoming.next().await;

    let server_response: definitions::ServerResponse = if let Some(Ok(Message::Text(text))) = final_response {
        match serde_json::from_str(&text) {
            Ok(data) => data,
            Err(_) => {
                return Err(definitions::HandshakeError::SerializationError);
            }
        }
    } else {
        return Err(definitions::HandshakeError::ServerError);
    };

    server_response.status_code
}

pub async fn auth_handshake_server(
    outgoing: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
    incoming: &mut SplitStream<WebSocketStream<TcpStream>>,
    session_token: String
) -> Result<(definitions::ClientHandshakeStart, definitions::HandshakeStatus), definitions::HandshakeError> {
    let first_msg = incoming.next().await;

    let join_request: definitions::ClientHandshakeStart = if let Some(Ok(Message::Text(text))) = first_msg {
        match serde_json::from_str(&text) {
            Ok(data) => data,
            Err(_) => {
                return Err(definitions::HandshakeError::SerializationError);
            }
        }
    } else {
        return Err(definitions::HandshakeError::ClientError);
    };

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32) // Length of random string
        .map(char::from)
        .collect();

    let server_challenge = definitions::ServerChallenge {
        nonce: random_string.clone(),
        version: definitions::VERSION
    };

    let server_challenge_message = Message::Text(serde_json::to_string(&server_challenge).unwrap().into());

    outgoing.send(server_challenge_message).await
        .map_err(|_| definitions::HandshakeError::SendFailed)?;

    let challenge_response = incoming.next().await;
    let final_auth: definitions::ClientHandshakeFinal = if let Some(Ok(Message::Text(text))) = challenge_response {
        match serde_json::from_str(&text) {
            Ok(data) => data,
            Err(_) => {
                return Err(definitions::HandshakeError::SerializationError);
            }
        }
    } else {
        return Err(definitions::HandshakeError::ClientError);
    };
    
    let nonce_string = random_string.clone();
    let correct_signature = verify_signature(&nonce_string, &final_auth.signed_message, &join_request.public_key).await;

    if correct_signature == false {
        return Err(definitions::HandshakeError::BadCryptography);
    }

    let final_result = definitions::ServerResponse {
        session_token: session_token,
        status_code: Ok(definitions::HandshakeStatus::Complete),
    };

    outgoing.send(
        Message::Text(
            serde_json::to_string(&final_result)
                .map_err(|_| definitions::HandshakeError::SerializationError)?
                .into())
    ).await.map_err(|_| definitions::HandshakeError::SendFailed)?;

    Ok((join_request, definitions::HandshakeStatus::Complete))
}