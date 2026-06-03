use ed25519_dalek::{VerifyingKey, ed25519::Error};

pub fn vec_to_verifying_key(vec: Vec<u8>) -> Result<VerifyingKey, Error> {
    if vec.len() != 32 {
        return Err(Error::new());
    } 
    let array: [u8; 32] = vec.try_into().map_err(|_| Error::new())?;
    Ok(VerifyingKey::from_bytes(&array)?)
}

pub fn size_to_descriptor(size: u64) -> String {
    match size {
        0..1_000 => format!("{size}B"),
        1_000..1_000_000 => format!("{}KB", size / 1_000),
        1_000_000..1_000_000_000 => format!("{}MB", size / 1_000_000),
        _ => format!("{}GB", size / 1_000_000_000),
    }
}
