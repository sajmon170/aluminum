use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};
use snow::TransportState;
use bytes::{Bytes, BytesMut};

pub struct NoiseCodec {
    framing_codec: LengthDelimitedCodec,
    noise: TransportState,
}

impl NoiseCodec {
    pub fn new(noise: TransportState) -> NoiseCodec {
        NoiseCodec {
            framing_codec: LengthDelimitedCodec::builder()
                .length_field_type::<u16>()
                .new_codec(),
            noise,
        }
    }
}

impl Encoder<Bytes> for NoiseCodec {
    type Error = std::io::Error;

    fn encode(&mut self, data: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut buf = vec![0; 65535];
        let len = self.noise.write_message(&data, &mut buf).unwrap();
        buf.truncate(len);
        self.framing_codec.encode(Bytes::from(buf), dst)
    }
}

impl Decoder for NoiseCodec {
    type Item = Bytes;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let result = self.framing_codec.decode(src)?;

        match result {
            Some(frame) => {
                let mut buf = vec![0; 65535];
                let len = self.noise.read_message(&frame, &mut buf).unwrap();
                buf.truncate(len);
                Ok(Some(Bytes::from(buf)))
            }
            None => Ok(None),
        }
    }
}
