use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};
use snow::TransportState;
use serde::{Serialize, de::DeserializeOwned};
use postcard::{from_bytes, to_allocvec};
use bytes::{Bytes, BytesMut};
use std::marker::PhantomData;

pub struct NoiseCodec<T> {
    decoding_type: PhantomData<T>,
    framing_codec: LengthDelimitedCodec,
    noise: TransportState,
}

impl<T> NoiseCodec<T> {
    pub fn new(noise: TransportState) -> NoiseCodec<T> {
        NoiseCodec::<T> {
            decoding_type: PhantomData,
            framing_codec: LengthDelimitedCodec::builder()
                .length_field_type::<u16>()
                .new_codec(),
            noise,
        }
    }
}

impl<T> Encoder<T> for NoiseCodec<T>
where T: Serialize + DeserializeOwned {
    type Error = std::io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let serialized: Vec<u8> = to_allocvec(&item).unwrap();
        //let mut buf = BytesMut::with_capacity(65535);
        let mut buf = vec![0; 65535];
        let len = self.noise.write_message(&serialized, &mut buf).unwrap();
        buf.truncate(len);
        self.framing_codec.encode(Bytes::from(buf), dst)
    }
}

impl<T> Decoder for NoiseCodec<T>
where T: Serialize + DeserializeOwned {
    type Item = T;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let result = self.framing_codec.decode(src)?;

        match result {
            Some(frame) => {
                let mut buf = vec![0; 65535];
                let len = self.noise.read_message(&frame, &mut buf).unwrap();
                buf.truncate(len);
                Ok(Some(from_bytes::<Self::Item>(&buf).unwrap()))
            }
            None => Ok(None),
        }
    }
}

