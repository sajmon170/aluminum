use bytes::{Bytes, BytesMut};
use snow::TransportState;
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

pub struct NoiseFrameCodec {
    framing_codec: LengthDelimitedCodec,
    noise: TransportState
}

impl NoiseFrameCodec {
    pub fn new(noise: TransportState) -> Self {
        Self {
            framing_codec: LengthDelimitedCodec::builder()
                .length_field_type::<u16>()
                .new_codec(),
            noise,
        }
    }
}

impl Encoder<Bytes> for NoiseFrameCodec {
    type Error = std::io::Error;

    fn encode(
        &mut self,
        data: Bytes,
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        let mut buf = vec![0; 65535];
        let len = self.noise.write_message(&data, &mut buf).unwrap();
        buf.truncate(len);
        self.framing_codec.encode(Bytes::from(buf), dst)
    }
}

impl Decoder for NoiseFrameCodec {
    type Item = Bytes;
    type Error = std::io::Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
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

pub struct NoiseCodec {
    noise: NoiseFrameCodec
}

impl NoiseCodec {
    pub fn new(noise: TransportState) -> Self {
        Self { noise: NoiseFrameCodec::new(noise) }
    }

    pub fn get_noise(&self) -> &TransportState {
        &self.noise.noise
    }
}

impl Encoder<Bytes> for NoiseCodec {
    type Error = std::io::Error;

    fn encode(
        &mut self,
        data: Bytes,
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        for chunk in data.chunks(65535) {
            self.noise.encode(chunk.to_owned().into(), dst)?;
        }

        Ok(())
    }
}

impl Decoder for NoiseCodec {
    type Item = Bytes;
    type Error = std::io::Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let mut result = BytesMut::with_capacity(65535);

        if src.is_empty() {
            return Ok(None);
        }

        while src.len() > 0 {
            let len = u16::from_be_bytes(src[..2].try_into().unwrap()) as usize
                + std::mem::size_of::<u16>();

            let mut frame = src.split_to(len);

            match self.noise.decode(&mut frame)? {
                Some(decoded) => result.extend_from_slice(decoded.as_ref()),
                None => return Ok(None)
            }
        }

        Ok(Some(Bytes::from(result)))
    }
}
