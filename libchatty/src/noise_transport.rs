use crate::asymmetric_codec::AsymmetricMessageCodec;
use crate::noise_session::NoiseSocket;
use futures::{sink::Sink, stream::Stream};
use pin_project::pin_project;
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};
use std::pin::Pin;

#[pin_project]
pub struct NoiseTransport<T, U, V>(
    #[pin] Framed<NoiseSocket<T>, AsymmetricMessageCodec<U, V>>,
);

impl<T, U, V> NoiseTransport<T, U, V>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    pub fn new(socket: NoiseSocket<T>) -> Self {
        Self(Framed::new(socket, AsymmetricMessageCodec::<U, V>::new()))
    }

    pub fn codec(&self) -> &AsymmetricMessageCodec<U, V> {
        self.0.codec()
    }

    pub fn codec_mut(&mut self) -> &mut AsymmetricMessageCodec<U, V> {
        self.0.codec_mut()
    }

    pub fn codec_pin_mut(self: Pin<&mut Self>) -> &mut AsymmetricMessageCodec<U, V> {
        let this = self.project();
        this.0.codec_pin_mut()
    }

    pub fn get_ref(&self) -> &NoiseSocket<T> {
        self.0.get_ref()
    }

    pub fn get_mut(&mut self) -> &mut NoiseSocket<T> {
        self.0.get_mut()
    }

    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut NoiseSocket<T>> {
        let this = self.project();
        this.0.get_pin_mut()
    }
}

impl<T, U, V> Sink<U> for NoiseTransport<T, U, V>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error = <AsymmetricMessageCodec<U, V> as Encoder<U>>::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.0.poll_ready(cx)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.0.poll_flush(cx)
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.0.poll_close(cx)
    }

    fn start_send(
        self: Pin<&mut Self>,
        item: U,
    ) -> Result<(), Self::Error> {
        let this = self.project();
        this.0.start_send(item)
    }
}

impl<T, U, V> Stream for NoiseTransport<T, U, V>
where
    T: AsyncRead + AsyncWrite + Unpin,
    U: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Item = Result<
        <AsymmetricMessageCodec<U, V> as Decoder>::Item,
        <AsymmetricMessageCodec<U, V> as Decoder>::Error,
    >;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        this.0.poll_next(cx)
    }
}

