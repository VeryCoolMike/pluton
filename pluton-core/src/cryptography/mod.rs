use argon2::{
    password_hash::{
        rand_core::OsRng, SaltString
    },
    {Argon2, Algorithm, Version, Params},
};
use ed25519_dalek::{SigningKey, Signature, Signer, VerifyingKey, Verifier};
use aes_gcm::{
    aead::{consts::U12, Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce
};

use crate::account_management;
use crate::helper;

// Returns a 
// 1. ciphertext
// 2. nonce
// 3. salt
// 4. public key
type KeyPairInformation = (Vec<u8>, Vec<u8>, SaltString, VerifyingKey);

pub async fn generate_key_pair(password: &str) -> anyhow::Result<KeyPairInformation> {
    // Generating the password hash with Argon2
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(
        32 * 1024, // 32 MiB memory cost
        2,         // time cost (number of iterations)
        1,         // parallelism (lanes)
        None,
    ).map_err(|e| anyhow::anyhow!("Argon2 params error: {:?}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key_bytes = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt.as_ref().as_bytes(), &mut key_bytes)
        .map_err(|e| anyhow::anyhow!("Argon2 error: {:?}", e))?;

    // Generating the keypair (public and private key)
    let mut csprng = OsRng;
    let signing_key: SigningKey = SigningKey::generate(&mut csprng); // The private key
    let verifying_key: VerifyingKey = signing_key.verifying_key(); // The public key
    
    // Now onto encrypting the private key
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, signing_key.as_bytes().as_ref())
        .map_err(|e| anyhow::anyhow!("AES-GCM error: {:?}", e))?;
    

    Ok((ciphertext, nonce.to_vec(), salt, verifying_key))
}

// Ciphertext and nonce are given as base64
// The bool can be either true or false, Err only represents an internal error
pub async fn check_password(salt: String, ciphertext: String, nonce: String, password: String) -> anyhow::Result<bool> {

    let params = Params::new(
        32 *1024, // 32 MiB memory cost
        2, // time cost (number of iterations)
        1, // parallelism (lanes)
        None,
    ).map_err(|e| anyhow::anyhow!("{}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key_bytes = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt.as_ref(), &mut key_bytes)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let ciphertext_bytes = helper::base64::from_base64(ciphertext);

    let nonce_bytes = helper::base64::from_base64(nonce);
    let final_nonce: Nonce<U12> = *Nonce::from_slice(&nonce_bytes);

    Ok(cipher.decrypt(&final_nonce, ciphertext_bytes.as_ref()).is_ok())
}

// Ciphertext and nonce are given as base64
pub async fn get_signing_key(password: &str) -> anyhow::Result<SigningKey> {
    let cfg: account_management::Account = account_management::get_account().await?;

    let params = Params::new(
        32 *1024, // 32 MiB memory cost
        2, // time cost (number of iterations)
        1, // parallelism (lanes)
        None,
    ).map_err(|e| anyhow::anyhow!("{}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key_bytes = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), cfg.salt.as_ref(), &mut key_bytes)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let ciphertext_bytes = helper::base64::from_base64(cfg.ciphertext);

    let nonce_bytes = helper::base64::from_base64(cfg.nonce);
    let final_nonce: Nonce<U12> = *Nonce::from_slice(&nonce_bytes);

    let decrypted_vec = cipher.decrypt(&final_nonce, ciphertext_bytes.as_ref())
        .map_err(|_| anyhow::anyhow!("Invalid password or corrupted data"))?;

    let byte_array: [u8; 32] = decrypted_vec.try_into()
        .map_err(|_| anyhow::anyhow!("Decrypted key was not 32 bytes"))?;

    let signing_key = SigningKey::from(byte_array);

    Ok(signing_key)
}

pub async fn sign_message(message: &str, signing_key: &SigningKey) -> Signature {
    let signature: Signature = signing_key.sign(message.as_bytes());

    signature
}

pub async fn verify_signature(message: &str, signature: &Signature, verifying_key: &VerifyingKey) -> bool  {
    verifying_key.verify(message.as_bytes(), signature).is_ok()
}
