use std::{fs::create_dir, io::prelude::*, time::{SystemTime, UNIX_EPOCH}};
use anyhow::{anyhow, Context};
use ed25519_dalek::{SigningKey, Verifier, VerifyingKey};
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use rand::{Rng, RngCore};

use tokio::{net::{TcpListener, TcpStream}, sync::{broadcast, Mutex}};
use futures_util::{StreamExt, SinkExt};

use crate::{cryptography::{self, check_password, get_signing_key}, helper::{self, base64}, networking::definitions::{self, home::UserProfile, UserOverview}};

#[derive(Serialize, Deserialize, Default)]
pub struct Account {
    #[serde(default)]
    pub account_id: String, // 8 random u8 values
    pub ciphertext: String, // base64
    pub nonce: String, // base64
    pub salt: String, // salt.as_str()
    pub verifying_key: String, // base64
    pub username: String,
    pub address: String,
    pub servers: String // base64
}

#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub warning: String,
    pub current_account: String, // matches account_id
    pub accounts: Vec<Account>
}

#[derive(Serialize, Deserialize, Debug)]
struct Servers {
    servers: Vec<String>
}

pub async fn get_account() -> anyhow::Result<Account> {
    let cfg: Settings = if let Ok(settings) = confy::load("pluton", None) {
        settings
    } else {
        return Err(anyhow::anyhow!("Config could not be found"))
    };

    for account in cfg.accounts {
        if account.account_id == cfg.current_account {
            return Ok(account);
        }
    }

    Err(anyhow::anyhow!("[account_management] Account could not be found"))
}

pub async fn get_servers() -> anyhow::Result<Vec<String>> {
    let cfg: Account = get_account().await?;

    let b64_input = cfg.servers;

    let decoded_bytes = base64::from_base64(b64_input);
    let decoded_string = std::str::from_utf8(&decoded_bytes).unwrap();
    println!("{:?}", decoded_string);

    let servers_json: Servers = serde_json::from_str(std::str::from_utf8(&decoded_bytes).unwrap())?;

    let result: Vec<String> = servers_json.servers.clone();

    println!("{:?}", result);

    Ok(result)
}


// This function will return Err if anything is incorrect such as incorrect password, internal
// errors, etc...
pub async fn sign_in(username: String, password: String) -> anyhow::Result<()> {
    let mut cfg: Settings = if let Ok(settings) = confy::load("pluton", None) {
        settings
    } else {
        return Err(anyhow::anyhow!("Config could not be found"))
    };

    for account in cfg.accounts.iter() {
        if account.username == username {
            match check_password(
                account.salt.clone(),
                account.ciphertext.clone(),
                account.nonce.clone(),
                password
            ).await {
                Ok(valid) => {
                    if valid {
                        cfg.current_account = account.account_id.clone();

                        confy::store("pluton", None, &cfg)
                            .map_err(|_| anyhow::anyhow!("Unable to save settings"))?;
                        return Ok(());
                    } else {
                        return Err(anyhow::anyhow!("Password is incorrect for account"));
                    }
                },
                Err(e) => {
                    eprintln!("[sign_in] check_password error: {e}");
                    return Err(anyhow::anyhow!(e));
                }
            }
        }
    }
    
    Err(anyhow::anyhow!("User with name could not be found"))
}

// This function will return Err if anything is incorrect such as already existing account, etc...
pub async fn sign_up(username: String, password: String, home_address: String) -> anyhow::Result<()> {
    let mut cfg: Settings = confy::load("pluton", None)
        .context("failed to load pluton config")?;

    cfg.warning = "WARNING! Changing settings in pluton.toml manually can cause permanent account loss!\nAll settings that should be changed can be changed in the Pluton GUI safely!".to_string();

    let key_pair = cryptography::generate_key_pair(&password).await;
    if let Ok(data) = key_pair {
        let mut id = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut id);


        let account = Account { 
            account_id: helper::base64::to_base64(id.to_vec()),
            ciphertext: helper::base64::to_base64(data.0),
            nonce: helper::base64::to_base64(data.1),
            salt: data.2.to_string(),
            verifying_key: helper::base64::to_base64(data.3.as_bytes().to_vec()),
            username: username.clone(),
            address: home_address.clone(),
            servers: String::new()
        };

        cfg.current_account = account.account_id.clone();

        cfg.accounts.push(account);


        confy::store("pluton", None, cfg)
            .map_err(|_| anyhow::anyhow!("Unable to save settings"))?;
        let path = confy::get_configuration_file_path("pluton", None)
            .map_err(|_| anyhow::anyhow!("Unable to get configuration path file"))?;

        if let Some(dir) = path.parent() {
            let warning = std::fs::File::create(dir.join("WARNING.txt"));
            if let Ok(mut warning_file) = warning {
                if let Err(e) = warning_file.write_all(
                    b"WARNING! Changing settings in pluton.toml manually can cause permanent account loss!\nAll settings that should be changed can be changed in the Pluton GUI safely!"
                ) {
                    println!("[WARNING] Unable to create warning, (non-fatal): {e}");
                }
            } else {
                println!("[WARNING] Unable to create warning file (non-fatal)");
            }
        }


        println!("{}", path.display());
    }

    if home_address != String::new() {
        let signing_key = cryptography::get_signing_key(&password).await?;
        join_home(home_address, signing_key).await?;
    }

    sign_in(username, password).await
}

pub async fn login(username: String, password: String, home_address: String) -> anyhow::Result<()> {
    if check_account_exists().await {
        sign_in(username, password).await
    } else {
        sign_up(username, password, home_address).await
    }
}

// TODO: DOESNT WORK FOR SOME REASON
pub async fn join_home(server_addr: String, signing_key: SigningKey) -> anyhow::Result<()> {
    let cfg: Account = get_account().await?;

    let client = reqwest::Client::new();

    let user_profile = definitions::home::UserProfile {
        username: cfg.username,
        biography: String::new()
    };

    let current_time = SystemTime::now();

    let time_from_epoch = current_time.duration_since(UNIX_EPOCH)
        .map_err(|_| anyhow::anyhow!("time went backwards"))?
        .as_secs();

    let account_creation_request = definitions::home::AccountCreation {
        profile: user_profile,
        timestamp: time_from_epoch as i64
    };

    let payload_string = serde_json::to_string(&account_creation_request)?;

    let signed_request = definitions::home::SignedRequest {
        payload: payload_string.clone(), 
        verifying_key: helper::base64::base64_to_base64url(cfg.verifying_key),
        signature: helper::base64::to_base64url(cryptography::sign_message(&payload_string, &signing_key).await.to_bytes().into())
    };

    let res = client.post(server_addr + "/create_profile")
        .json(&signed_request)
        .send()
        .await?;

    Ok(())
}

// RETURNS ERROR WHEN IT SHOULDNT OR SOMETHING, ANYWAY ITS BROKEN!
pub async fn request_home_info(server_addr: String, verifying_key: VerifyingKey) -> anyhow::Result<UserProfile> {
    if server_addr == String::new() {
        return Err(anyhow::anyhow!("No home provided"))
    }
    let client = reqwest::Client::new();

    let response = client.get(server_addr + "/profile/" + &helper::base64::to_base64url(verifying_key.to_bytes().to_vec()))
        .send()
        .await?;

    let response_json: UserProfile = response.json::<UserProfile>().await?;

    Ok(response_json)
}

pub async fn check_password_strength(password: String) -> String {
    let common_passwords = ["123456", "123456789", "12345678", "password", "qwerty123", "qwerty1", "111111", "12345", "sercret", "123123"];

    let mut result = String::new();
    if password.len() < 8 {
        result += "[WARNING] You are using a short password!\n";
    }
    if common_passwords.contains(&password.as_str()) {
        result += "[WARNING] You are using a common password!\n";
    }
    if !password.chars().any(|c| c.is_numeric()) {
        result += "[WARNING] You are using a password without any numbers!\n";
    }

    if password.chars().any(|c| !c.is_alphanumeric()) {
        result += "[WARNING] You are using a password without any special characters!\n";
    }


    result
}

pub async fn check_account_exists() -> bool {
    let path: PathBuf = confy::get_configuration_file_path("pluton", None)
        .expect("Error opening settings");
    Path::new(&path).exists()
}
