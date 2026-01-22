use ed25519_dalek::{VerifyingKey, ed25519::Error};

pub fn vec_to_verifying_key(vec: Vec<u8>) -> Result<VerifyingKey, Error> {
    if vec.len() != 32 {
        return Err(Error::new());
    } 
    let array: [u8; 32] = vec.try_into().map_err(|_| Error::new())?;
    Ok(VerifyingKey::from_bytes(&array)?)
}