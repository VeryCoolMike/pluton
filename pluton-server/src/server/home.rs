use ed25519_dalek::{VerifyingKey};
use reqwest::{Client};
use pluton_core::{cryptography, helper, networking::definitions};
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn get_account_info(verifying_key: VerifyingKey, server_addr: String) -> Result<String, anyhow::Error> {
    let client = reqwest::Client::new();

    let res = client.get(server_addr + "/profile/" + &helper::base64::to_base64url(verifying_key.as_bytes().as_slice().to_vec()))
        .send()
        .await?;

    

    Ok(String::new())
}
