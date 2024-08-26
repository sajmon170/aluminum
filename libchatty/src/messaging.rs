use serde::{Serialize, Deserialize};
use ed25519_dalek::VerifyingKey;
use std::net::SocketAddr;

#[derive(Serialize, Deserialize, Debug)]
pub enum RelayMessage {
    GetUser(VerifyingKey),
    UserAddress(SocketAddr),
    Bye
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessage {
    Send(String)
}
