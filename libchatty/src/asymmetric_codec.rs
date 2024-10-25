use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};
use serde::{de::DeserializeOwned, Serialize};
use bytes::{Bytes, BytesMut};
use postcard::{from_bytes, to_allocvec};
use std::marker::PhantomData;

pub struct AsymmetricMessageCodec<U, V> {
    framing_codec: LengthDelimitedCodec,
    encoded_type: PhantomData<U>,
    decoded_type: PhantomData<V>
}

impl<U, V> Encoder<U> for AsymmetricMessageCodec<U, V>
where
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error = std::io::Error;

    fn encode(
        &mut self,
        item: U,
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        let serialized: Vec<u8> = to_allocvec(&item).unwrap();
        self.framing_codec.encode(Bytes::from(serialized), dst)
    }
}

impl<U, V> Decoder for AsymmetricMessageCodec<U, V>
where
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Item = V;
    type Error = std::io::Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let result = self.framing_codec.decode(src)?;

        match result {
            Some(data) => Ok(Some(from_bytes::<Self::Item>(&data).unwrap())),
            None => Ok(None),
        }
    }
}

impl<U, V> AsymmetricMessageCodec<U, V>
where
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn new() -> Self {
        Self {
            encoded_type: PhantomData,
            decoded_type: PhantomData,
            framing_codec: LengthDelimitedCodec::new()
        }
    }
}
