use crate::noise_codec::NoiseCodec;
use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use snow::{Builder, HandshakeState, Keypair};
use std::{
    error::Error,
    fmt::{Display, Formatter},
    marker::PhantomData,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub type NoiseStream<T, U> = Framed<T, NoiseCodec<U>>;
type Key = Vec<u8>;

pub enum NoiseSelfType {
    N,
    I,
    X,
    K,
}

impl Display for NoiseSelfType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        match self {
            NoiseSelfType::N => write!(f, "N"),
            NoiseSelfType::I => write!(f, "I"),
            NoiseSelfType::X => write!(f, "X"),
            NoiseSelfType::K => write!(f, "K"),
        }
    }
}

pub enum NoisePeerType {
    N,
    I,
    X,
    K(Key),
}

impl Display for NoisePeerType {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        match self {
            NoisePeerType::N => write!(f, "N"),
            NoisePeerType::I => write!(f, "I"),
            NoisePeerType::X => write!(f, "X"),
            NoisePeerType::K(_) => write!(f, "K"),
        }
    }
}

pub struct NoiseTransportBuilder<T, U>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
{
    my_keys: Keypair,
    my_type: NoiseSelfType,
    peer_type: NoisePeerType,
    stream: T,
    message_type: PhantomData<U>,
}

impl<T, U> NoiseTransportBuilder<T, U>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
{
    pub fn new(my_keys: Keypair, stream: T) -> NoiseTransportBuilder<T, U> {
        NoiseTransportBuilder {
            my_keys,
            my_type: NoiseSelfType::X,
            peer_type: NoisePeerType::X,
            stream,
            message_type: PhantomData,
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
        self,
    ) -> Result<Framed<T, NoiseCodec<U>>, Box<dyn Error + Send + Sync>> {
        let protocol = format!(
            "Noise_{}{}_25519_ChaChaPoly_BLAKE2b",
            self.my_type, self.peer_type
        );

        let mut noise =
            Builder::new(protocol.parse().unwrap()).local_private_key(&self.my_keys.private);

        if let NoisePeerType::K(ref key) = self.peer_type {
            noise = noise.remote_public_key(key);
        }

        let noise = noise.build_initiator()?;
        Ok(handshake(noise, self.stream).await?)
    }

    pub async fn build_as_responder(
        self,
    ) -> Result<Framed<T, NoiseCodec<U>>, Box<dyn Error + Send + Sync>> {
        let protocol = format!(
            "Noise_{}{}_25519_ChaChaPoly_BLAKE2b",
            self.peer_type, self.my_type
        );

        let mut noise =
            Builder::new(protocol.parse().unwrap()).local_private_key(&self.my_keys.private);

        if let NoisePeerType::K(ref key) = self.peer_type {
            noise = noise.remote_public_key(key);
        }

        let noise = noise.build_responder()?;
        Ok(handshake(noise, self.stream).await?)
    }
}

async fn handshake<T, U>(
    mut noise: HandshakeState,
    stream: T,
) -> Result<Framed<T, NoiseCodec<U>>, Box<dyn Error + Send + Sync>>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
{
    let framing_codec = LengthDelimitedCodec::builder()
        .length_field_type::<u16>()
        .new_codec();

    let mut framed = Framed::new(stream, framing_codec);

    while !noise.is_handshake_finished() {
        let mut buf = vec![0u8; 65535];
        // Note: We cannot use Tokio Bytes directly since the snow crate expects
        // a [u8] argument with all elements filled in. Converting the Bytes
        // into a format accepted by snow would introduce non-idiomatic code with
        // additional performance penalties.

        if noise.is_my_turn() {
            let len = noise.write_message(&[], &mut buf)?;
            buf.truncate(len);
            framed.send(Bytes::from(buf.clone())).await?;
        } else {
            let msg = framed.next().await.unwrap()?;
            let msg = msg.to_vec();
            noise.read_message(&msg, &mut buf)?;
        }
    }

    let noise = noise.into_transport_mode()?;
    let parts = framed.into_parts();

    Ok(Framed::new(parts.io, NoiseCodec::new(noise)))
}
