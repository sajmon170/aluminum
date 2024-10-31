use crate::messaging::UserMessage;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserMetadata {
    pub name: String,
    pub surname: String,
    pub nickname: String,
    pub description: String,
    pub version: u32, // Used by other clients for versioning metadata
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    metadata: UserMetadata,
    public_key: VerifyingKey,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Relay {
    pub addr: SocketAddr,
    #[serde(with = "crate::base64_codec")]
    pub public_key: VerifyingKey,
}

impl Relay {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let relay = std::fs::read_to_string(path)?;
        Ok(toml::from_str::<Relay>(&relay).unwrap())
    }

    pub fn save(&self, path: &Path) {
        let serialized = toml::to_string(self).unwrap();
        fs::write(path, serialized).unwrap();
    }
}

impl User {
    pub fn load_file(path: &Path) -> User {
        let serialized = fs::read(path).unwrap();
        postcard::from_bytes(&serialized).unwrap()
    }

    pub fn save_file(&self, path: &Path) {
        let serialized = postcard::to_allocvec(self).unwrap();
        fs::write(path, serialized).unwrap();
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Myself {
    pub metadata: UserMetadata,
    pub private_key: SigningKey,
}

impl Myself {
    pub fn share(&self) -> User {
        User {
            metadata: self.metadata.clone(),
            public_key: self.private_key.verifying_key(),
        }
    }

    pub fn get_public_key(&self) -> VerifyingKey {
        self.private_key.verifying_key()
    }

    pub fn get_private_key(&self) -> &SigningKey {
        &self.private_key
    }
}

#[derive(Clone, Debug)]
pub struct IdentityBuilder {
    name: String,
    surname: String,
    nickname: String,
    description: String,
    private_key: Option<SigningKey>
}

impl IdentityBuilder {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            surname: String::new(),
            nickname: String::new(),
            description: String::new(),
            private_key: None
        }
    }
    
    pub fn name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn surname(mut self, surname: String) -> Self {
        self.surname = surname;
        self
    }

    pub fn nickname(mut self, nickname: String) -> Self {
        self.nickname = nickname;
        self
    }

    pub fn description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    pub fn with_key(mut self, private_key: SigningKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn build(self) -> Myself {
        Myself {
            metadata: UserMetadata {
                name: self.name,
                surname: self.surname,
                nickname: self.nickname,
                description: self.description,
                version: 0
            },
            private_key: self.private_key.unwrap_or_else(|| {
                println!("Here!");
                let mut csprng = OsRng;
                SigningKey::generate(&mut csprng)
            })
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserDb {
    path: PathBuf,
    pub myself: Myself, // TODO: Make this a list of multiple identities
    pub remote: HashMap<VerifyingKey, UserMetadata>,
    pub messages: HashMap<VerifyingKey, Vec<UserMessage>>,
}

// TODO: Make this safe - implement error handling!
// Advice - use some crate for merging multiple error types
// into a single one
impl UserDb {
    pub fn new(path: PathBuf, myself: Myself) -> Self {
        Self {
            path,
            myself,
            remote: HashMap::new(),
            messages: HashMap::new(),
        }
    }

    pub fn add_user(&mut self, user: User) {
        self.remote.insert(user.public_key, user.metadata);
    }

    pub fn save(&self) {
        let serialized = postcard::to_allocvec(&self).unwrap();
        fs::write(&self.path, serialized).unwrap();
    }

    pub fn load(path: &Path) -> Self {
        let serialized = fs::read(path).unwrap();
        postcard::from_bytes(&serialized).unwrap()
    }

    pub fn get_user_data(&self) -> User {
        self.myself.share()
    }

    pub fn get_master_key(&self) -> &SigningKey {
        &self.myself.private_key
    }

    pub fn find_user_by_name(&self, nickname: &str) -> Option<&VerifyingKey> {
        self.remote
            .iter()
            .filter(|(_, x)| x.nickname == nickname)
            .map(|(public_key, _)| public_key)
            .next()
    }
}

impl Drop for UserDb {
    fn drop(&mut self) {
        self.save();
    }
}
