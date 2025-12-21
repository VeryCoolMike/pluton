use std::io::prelude::*;
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};

use crate::{cryptography::{self, check_password}, helper::{self, base64}};

#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub account_made: bool,
    pub ciphertext: String, // base64
    pub nonce: String, // base64
    pub salt: String, // salt.as_str()
    pub verifying_key: String, // base64
    pub username: String,
    pub servers: String // base64
}

#[derive(Serialize, Deserialize, Debug)]
struct Servers {
    servers: Vec<String>
}

pub async fn get_servers() -> Result<Vec<String>, String> {
    let cfg: Settings = if let Ok(settings) = confy::load("pluton", None) {
        settings
    } else {
        return Ok(Vec::new());
    };

    let b64_input = cfg.servers;

    let decoded_bytes = base64::from_base64(b64_input);
    let decoded_string = std::str::from_utf8(&decoded_bytes).unwrap();
    println!("{:?}", decoded_string);

    let servers_json: Servers = serde_json::from_str(std::str::from_utf8(&decoded_bytes).unwrap())
        .map_err(|e| format!("{e}"))?;

    let result: Vec<String> = servers_json.servers.clone();

    println!("{:?}", result);

    Ok(result)
}


pub async fn sign_in(password: String) -> bool {
    let cfg: Settings = if let Ok(settings) = confy::load("pluton", None) {
        settings
    } else {
        eprintln!("[sign_in] Unable to load settings");
        return false;
    };

    if !cfg.account_made {
        eprintln!("[sign_in] account_made is false");
        return false;
    }

    eprintln!("[sign_in] stored username: '{}', ciphertext.len={}, nonce.len={}, salt.len={}",
        cfg.username,
        cfg.ciphertext.len(),
        cfg.nonce.len(),
        cfg.salt.len(),
    );
    
    match check_password(&password).await {
        Ok(valid) => valid,
        Err(e) => {
            eprintln!("[sign_in] check_password error: {}", e);
            false
        }
    }

}

pub async fn sign_up(username: String, password: String) -> bool {
    let mut cfg: Settings = confy::load("pluton", None)
        .expect("Error opening settings");

    if cfg.account_made {
        println!("[WARNING] Account was attempted to be made although it already existed!");
        return false;
    }

    let key_pair = cryptography::generate_key_pair(&password).await;
    cfg.username = username.clone();
    if let Ok(data) = key_pair {
        // 1. Ciphertext
        // 2. Nonce
        // 3. Salt
        // 4. Verifying Key
        cfg.ciphertext = helper::base64::to_base64(data.0);
        cfg.nonce = helper::base64::to_base64(data.1);
        cfg.salt = data.2.to_string();
        cfg.verifying_key = helper::base64::to_base64(data.3.as_bytes().to_vec());
        cfg.account_made = true;
        cfg.servers = String::from("ewogICAgInNlcnZlcnMiOiBbImZvbyIsICJiYXIiLCAiYmF6Il0KfQ=="); // Temporary!
        confy::store("pluton", None, cfg)
            .expect("Unable to save settings");
        let path = confy::get_configuration_file_path("pluton", None)
            .expect("Unable to get configuration file path");

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

    sign_in(password).await
}

pub async fn login(username: String, password: String) -> bool{
    if check_account_exists().await {
         return sign_in(password).await;
    } else {
        return sign_up(username, password).await;
    }
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
