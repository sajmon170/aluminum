use crate::{messaging::AsymmetricMessageCodec, noise_codec::NoiseCodec};
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
use tracing::{event, Level};

pub type NoiseConnection<T, U, V> = Framed<T, AsymmetricMessageCodec<U, V>>;
pub type SymmetricNoiseConnection<T, U> = NoiseConnection<T, U, U>;
type Key = Vec<u8>;

pub enum NoiseSelfType {
    N,
    I,
    X,
    K,
}

// TODO - rewrite this using the strum macros
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

pub struct NoiseTransportBuilder<T, U, V>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    my_keys: Keypair,
    my_type: NoiseSelfType,
    peer_type: NoisePeerType,
    stream: T,
    send_type: PhantomData<U>,
    receive_type: PhantomData<V>,
}

impl<T, U, V> NoiseTransportBuilder<T, U, V>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn new(my_keys: Keypair, stream: T) -> NoiseTransportBuilder<T, U, V> {
        NoiseTransportBuilder {
            my_keys,
            my_type: NoiseSelfType::X,
            peer_type: NoisePeerType::X,
            stream,
            send_type: PhantomData,
            receive_type: PhantomData,
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
    ) -> Result<
        Framed<T, AsymmetricMessageCodec<U, V>>,
        Box<dyn Error + Send + Sync>,
    > {
        event!(Level::INFO, "Building as initiator");

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
        event!(Level::INFO, "Finished the Noise handshake");
        let codec = AsymmetricMessageCodec::<U, V>::new(noise);

        Ok(Framed::new(self.stream, codec))
    }

    pub async fn build_as_responder(
        mut self,
    ) -> Result<
        Framed<T, AsymmetricMessageCodec<V, U>>,
        Box<dyn Error + Send + Sync>,
    > {
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
        let codec = AsymmetricMessageCodec::<V, U>::new(noise);
        Ok(Framed::new(self.stream, codec))
    }
}

async fn handshake<T>(
    mut noise: HandshakeState,
    stream: &mut T,
) -> Result<NoiseCodec, Box<dyn Error + Send + Sync>>
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
        // an [u8] argument with all elements filled in. Converting the Bytes
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

    let noise = noise.into_transport_mode()?;
    Ok(NoiseCodec::new(noise))
}
