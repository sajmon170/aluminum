use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::net::SocketAddr;
use tokio_util::codec::{Decoder, Encoder};

use crate::noise_codec::NoiseCodec;
use bytes::{Bytes, BytesMut};
use postcard::{from_bytes, to_allocvec};
use std::marker::PhantomData;

#[derive(Serialize, Deserialize, Debug)]
pub enum RelayRequest {
    GetUser(VerifyingKey),
    Bye,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RelayResponse {
    UserAddress(SocketAddr),
    Ack,
}

pub enum UserFrame {
    Connect(VerifyingKey),
    Send(Message),
    Ack,
    Bye,
}

pub enum Message {
    Text(String)
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

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
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

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let result = self.noise_codec.decode(src)?;

        match result {
            Some(data) => {
                Ok(Some(from_bytes::<Self::Item>(&data).unwrap()))
            }
            None => Ok(None)
        }
    }
}

impl<T, U> AsymmetricMessageCodec<T, U> {
    pub fn new(noise_codec: NoiseCodec) -> Self {
        AsymmetricMessageCodec {
            encoded_type: PhantomData,
            decoded_type: PhantomData,
            noise_codec
        }
    }
}
