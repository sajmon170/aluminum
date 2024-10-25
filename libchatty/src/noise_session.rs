use crate::noise_codec::NoiseCodec;
use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use pin_project::pin_project;
use snow::{Builder, HandshakeState, Keypair, TransportState};
use std::error::Error;
use strum_macros::Display;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::{
    codec::{Framed, LengthDelimitedCodec},
    io::{CopyToBytes, SinkWriter, StreamReader},
};
use tracing::{event, Level};

type Key = Vec<u8>;

#[derive(Display, Debug)]
pub enum NoiseSelfType {
    N,
    I,
    X,
    K,
}

#[derive(Display, Debug)]
pub enum NoisePeerType {
    N,
    I,
    X,
    K(Key),
}

pub struct NoiseBuilder<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    my_keys: Keypair,
    my_type: NoiseSelfType,
    peer_type: NoisePeerType,
    stream: T,
}

impl<T> NoiseBuilder<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(my_keys: Keypair, stream: T) -> Self {
        Self {
            my_keys,
            my_type: NoiseSelfType::X,
            peer_type: NoisePeerType::X,
            stream,
        }
    }

    pub fn set_keys(mut self, keys: Keypair) -> Self {
        self.my_keys = keys;
        self
    }

    pub fn set_my_type(mut self, myself: NoiseSelfType) -> Self {
        self.my_type = myself;
        self
    }

    pub fn set_peer_type(mut self, peer: NoisePeerType) -> Self {
        self.peer_type = peer;
        self
    }

    pub async fn build_as_initiator(
        mut self,
    ) -> Result<NoiseSocket<T>, Box<dyn Error + Send + Sync>> {
        let protocol = format!(
            "Noise_{}{}_25519_ChaChaPoly_BLAKE2b",
            self.my_type, self.peer_type
        );

        let mut noise = Builder::new(protocol.parse().unwrap())
            .local_private_key(&self.my_keys.private);

        if let NoisePeerType::K(ref key) = self.peer_type {
            noise = noise.remote_public_key(key);
        }

        let noise = noise.build_initiator()?;
        let noise = handshake(noise, &mut self.stream).await?;

        Ok(NoiseSocket::new(self.stream, NoiseCodec::new(noise)))
    }

    pub async fn build_as_responder(
        mut self,
    ) -> Result<NoiseSocket<T>, Box<dyn Error + Send + Sync>> {
        let protocol = format!(
            "Noise_{}{}_25519_ChaChaPoly_BLAKE2b",
            self.peer_type, self.my_type
        );

        let mut noise = Builder::new(protocol.parse().unwrap())
            .local_private_key(&self.my_keys.private);

        if let NoisePeerType::K(ref key) = self.peer_type {
            noise = noise.remote_public_key(key);
        }

        let noise = noise.build_responder()?;
        let noise = handshake(noise, &mut self.stream).await?;

        Ok(NoiseSocket::new(self.stream, NoiseCodec::new(noise)))
    }
}

async fn handshake<T>(
    mut noise: HandshakeState,
    stream: &mut T,
) -> Result<TransportState, Box<dyn Error + Send + Sync>>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let framing_codec = LengthDelimitedCodec::builder()
        .length_field_type::<u16>()
        .new_codec();

    let mut framed = Framed::new(stream, framing_codec);

    event!(Level::INFO, "Beginning a handshake");

    while !noise.is_handshake_finished() {
        let mut buf = vec![0u8; 65535];
        // Note: We cannot use Tokio Bytes directly since the snow crate expects
        // an [u8] argument with all elements filled in. Converting Bytes
        // into a format accepted by snow would introduce non-idiomatic code
        // with additional performance penalties.

        if noise.is_my_turn() {
            event!(Level::INFO, "Trying to send a handshake message");
            let len = noise.write_message(&[], &mut buf)?;
            buf.truncate(len);
            framed.send(Bytes::from(buf.clone())).await?;
            event!(Level::INFO, "Sent handshake message");
        } else {
            event!(Level::INFO, "Trying to receive a handshake message");
            let msg = framed.next().await.unwrap()?;
            event!(Level::INFO, "Received handshake message");
            let msg = msg.to_vec();
            noise.read_message(&msg, &mut buf)?;
        }
    }

    Ok(noise.into_transport_mode()?)
}

#[pin_project]
pub struct NoiseSocket<T>(
    #[pin] SinkWriter<StreamReader<CopyToBytes<Framed<T, NoiseCodec>>, Bytes>>,
);

impl<T> NoiseSocket<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(stream: T, noise: NoiseCodec) -> Self {
        let framed = Framed::new(stream, noise);
        Self(SinkWriter::new(StreamReader::new(CopyToBytes::new(framed))))
    }

    fn deref(&self) -> &Framed<T, NoiseCodec> {
        self.0
            .get_ref()
            .get_ref()
            .get_ref()
    }

    fn get_noise(&self) -> &TransportState {
        self.deref()
            .codec()
            .get_noise()
    }

    pub fn get_remote_static(&self) -> Option<&[u8]> {
        self.get_noise()
            .get_remote_static()
    }
}

impl<T> AsyncRead for NoiseSocket<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.project();
        this.0.poll_read(cx, buf)
    }
}

impl<T> AsyncWrite for NoiseSocket<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        this.0.poll_write(cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.project();
        this.0.poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.project();
        this.0.poll_shutdown(cx)
    }
}
