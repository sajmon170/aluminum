use base64::prelude::*;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use serde::{Deserializer, Serializer};

pub fn serialize<S: Serializer>(
    v: &VerifyingKey,
    s: S,
) -> Result<S::Ok, S::Error> {
    let base64 = BASE64_STANDARD.encode(v.as_bytes());
    String::serialize(&base64, s)
}

pub fn deserialize<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<VerifyingKey, D::Error> {
    let base64 = String::deserialize(d)?;
    let decoded: Vec<u8> = BASE64_STANDARD
        .decode(base64.as_bytes())
        .map_err(|e| serde::de::Error::custom(e))?;
    VerifyingKey::try_from(&decoded[..32])
        .map_err(|e| serde::de::Error::custom(e))
}
