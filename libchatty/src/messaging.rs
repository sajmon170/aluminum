use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use enum_as_inner::EnumAsInner;
use chrono::{DateTime, Utc};
use crate::system::FileMetadata;

// TODO
// Rename RelayRequest to UserToRelayMessage
// Rename RelayResponse to RelayToUserMessage
// Rename PeerPacket to UserToUserMessage

#[derive(Clone, Serialize, Deserialize, Debug, EnumAsInner)]
pub enum RelayRequest {
    Register(VerifyingKey),
    GetUser(VerifyingKey),
    Ack,
    Bye,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumAsInner)]
pub enum RelayResponse {
    UserAddress(Option<SocketAddr>),
    AwaitConnection(VerifyingKey, SocketAddr),
    Ack,
}

#[derive(Clone, Serialize, Deserialize, Debug, EnumAsInner)]
pub enum PeerPacket {
    Send(PeerMessageData),
    Share(FileMetadata),
    GimmeFile,
    Ack,
    Bye,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PeerMessageData {
    Text(String),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UserMessage {
    pub author: VerifyingKey,
    pub content: PeerMessageData,
    pub timestamp: DateTime<Utc>,
}

impl UserMessage {
    pub fn new(peer: VerifyingKey, message: PeerMessageData) -> Self {
        Self {
            author: peer,
            content: message,
            timestamp: Utc::now(),
        }
    }
}
