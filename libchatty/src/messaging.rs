use ed25519_dalek::VerifyingKey;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::net::SocketAddr;
use tokio_util::codec::{Decoder, Encoder};

use crate::noise_codec::NoiseCodec;
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use postcard::{from_bytes, to_allocvec};
use std::marker::PhantomData;

// TODO
// Rename RelayRequest to UserToRelayMessage
// Rename RelayResponse to RelayToUserMessage

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RelayRequest {
    Register(VerifyingKey),
    GetUser(VerifyingKey),
    Ack,
    Bye,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RelayResponse {
    UserAddress(Option<SocketAddr>),
    AwaitConnection(VerifyingKey, SocketAddr),
    Ack,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PeerPacket {
    Send(PeerMessageData),
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

pub struct AsymmetricMessageCodec<T, U> {
    encoded_type: PhantomData<T>,
    decoded_type: PhantomData<U>,
    noise_codec: NoiseCodec,
}

impl<T, U> Encoder<T> for AsymmetricMessageCodec<T, U>
where
    T: Serialize + DeserializeOwned,
    U: Serialize + DeserializeOwned,
{
    type Error = std::io::Error;

    fn encode(
        &mut self,
        item: T,
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        let serialized: Vec<u8> = to_allocvec(&item).unwrap();
        self.noise_codec.encode(Bytes::from(serialized), dst)
    }
}

impl<T, U> Decoder for AsymmetricMessageCodec<T, U>
where
    T: Serialize + DeserializeOwned,
    U: Serialize + DeserializeOwned,
{
    type Item = U;
    type Error = std::io::Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let result = self.noise_codec.decode(src)?;

        match result {
            Some(data) => Ok(Some(from_bytes::<Self::Item>(&data).unwrap())),
            None => Ok(None),
        }
    }
}

impl<T, U> AsymmetricMessageCodec<T, U> {
    pub fn new(noise_codec: NoiseCodec) -> Self {
        AsymmetricMessageCodec {
            encoded_type: PhantomData,
            decoded_type: PhantomData,
            noise_codec,
        }
    }
}
