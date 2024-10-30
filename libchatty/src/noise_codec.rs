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
    framing: LengthDelimitedCodec,
    noise: NoiseFrameCodec
}

impl NoiseCodec {
    pub fn new(noise: TransportState) -> Self {
        Self {
            framing: LengthDelimitedCodec::new(),
            noise: NoiseFrameCodec::new(noise)
        }
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
        let mut noise_frames = BytesMut::with_capacity(65535);
        
        for chunk in data.chunks(65535) {
            self.noise.encode(chunk.to_owned().into(), &mut noise_frames)?;
        }

        self.framing.encode(Bytes::from(noise_frames), dst)?;

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
        if let Some(mut frames) = self.framing.decode(src)? {
            let mut result = BytesMut::with_capacity(65535);
            
            while frames.len() > 0 {
                let len = u16::from_be_bytes(frames[..2].try_into().unwrap()) as usize
                    + std::mem::size_of::<u16>();

                let mut frame = frames.split_to(len);

                let decoded = self.noise.decode(&mut frame)?
                    .ok_or(std::io::ErrorKind::InvalidData)?;

                result.extend_from_slice(&decoded);
            }

            Ok(Some(Bytes::from(result)))
        }
        else {
            Ok(None)
        }
    }
}
