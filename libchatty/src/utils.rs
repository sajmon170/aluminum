use std::{io, path::Path};
use tokio::fs::File;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;
use ed25519_dalek::{SigningKey, VerifyingKey};
use snow::Keypair;

pub fn ed25519_signing_to_x25519(key: &SigningKey) -> Vec<u8> {
    key.to_scalar_bytes().to_vec()
}

pub fn ed25519_verifying_to_x25519(key: &VerifyingKey) -> Vec<u8> {
    key.to_montgomery().to_bytes().to_vec()
}

pub fn ed25519_to_noise(key: &SigningKey) -> Keypair {
    Keypair {
        public: ed25519_verifying_to_x25519(&key.verifying_key()),
        private: ed25519_signing_to_x25519(key),
    }
}

pub async fn get_hash_from_path(path: &Path) -> io::Result<blake3::Hash> {
    let mut file = File::open(path).await?;
    get_hash_from_file(&mut file).await
}

pub async fn get_hash_from_file(file: &mut File) -> io::Result<blake3::Hash> {
    let mut stream = ReaderStream::new(file);
    let mut hasher = blake3::Hasher::new();

    while let Some(chunk) = stream.next().await {
        hasher.update(&chunk?);
    }

    Ok(hasher.finalize())
}
