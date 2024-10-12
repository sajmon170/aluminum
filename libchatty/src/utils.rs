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
