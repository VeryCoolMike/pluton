pub mod definitions;
pub mod auth_handshake;

use anyhow::{anyhow, Result};
use reqwest::multipart::{Form, Part};

pub async fn upload_file(
    server_host: String,
    session_id: String,
    file_name: String,
    data: Vec<u8>,
    mime_type: String,
) -> Result<u64> {
    let part = Part::bytes(data)
        .file_name(file_name)
        .mime_str(&mime_type)?;

    let form = Form::new()
        .text("session_id", session_id)
        .part("file", part);

    let url = format!("{}:6766/upload_file", server_host);

    let response = reqwest::Client::new()
        .post(&url)
        .multipart(form)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("upload failed: {}", response.status()));
    }

    Ok(response.json::<u64>().await?)
}

pub async fn download_file(
    server_host: String,
    file_id: u64
) -> Result<Vec<u8>> {
    let url = format!("{}:6766/download_file/{}", server_host, file_id.to_string());

    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("download failed: {}", response.status()));
    }

    Ok(response.bytes().await?.to_vec())
}

pub async fn download_file_meta(
    server_host: String,
    file_id: u64
) -> Result<definitions::FileDescriptor> {
    let url = format!("{}:6766/download_file_meta/{}", server_host, file_id.to_string());

    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("download failed: {}", response.status()));
    }

    Ok(response.json().await?)
}
