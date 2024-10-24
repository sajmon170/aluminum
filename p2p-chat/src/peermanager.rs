use libchatty::{
    identity::Myself,
    messaging::{PeerMessageData, PeerPacket, UserMessage},
    noise_session::*,
    utils,
};

use std::{error::Error, net::SocketAddr, time::Duration};

use ed25519_dalek::VerifyingKey;
use futures::{sink::SinkExt, stream::StreamExt};
use quinn::{Connection, ConnectionError, Endpoint};
use tokio::{sync::mpsc, time::sleep};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{event, Level};

use crate::connmanager::ConnMessage;

type PeerConnection = NoiseConnection<QuinnStream, PeerPacket, PeerPacket>;

pub enum P2pRole {
    Initiator,
    Responder,
}

struct PeerManager {
    identity: Myself,
    endpoint: Endpoint,
    peer_key: VerifyingKey,
    peer_addr: SocketAddr,
    token: CancellationToken,
    role: P2pRole,
    rx: mpsc::Receiver<PeerManagerCommand>,
    tx: mpsc::Sender<ConnMessage>,
}

type QuinnStream = tokio::io::Join<quinn::RecvStream, quinn::SendStream>;

impl PeerManager {
    async fn run(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        event!(Level::DEBUG, "Trying to hole-punch...");
        let mut conn = self.connect().await?;

        loop {
            tokio::select! {
                Some(Ok(PeerPacket::Send(msg))) = conn.next() => {
                    let text = match &msg {
                        PeerMessageData::Text(text) => text.clone()
                    };
                    event!(Level::INFO, "Received message: {text}");
                    self.tx.send(ConnMessage::UserMessage(
                        UserMessage::new(self.peer_key, msg)
                    )).await?;
                }
                Some(PeerManagerCommand::Send(msg)) = self.rx.recv() => {
                    let text = match &msg {
                        PeerMessageData::Text(text) => text.clone()
                    };
                    event!(Level::INFO, "Sending message: {text}");
                    conn.send(PeerPacket::Send(msg)).await?;
                }
                _ = self.token.cancelled() => { break; }
                else => { self.token.cancel() }
            }
        }

        Ok(())
    }

    async fn connect(
        &self,
    ) -> Result<PeerConnection, Box<dyn Error + Send + Sync>> {
        let (writer, reader) = {
            let (incoming, outgoing) =
                tokio::join!(self.accept_peer(), self.connect_to_peer());

            event!(Level::DEBUG, "Hole punch success");

            match self.role {
                P2pRole::Initiator => {
                    outgoing?.open_bi().await?
                }
                P2pRole::Responder => {
                    incoming?.accept_bi().await?
                }
            }
        };

        let stream = tokio::io::join(reader, writer);
        let stream = self.upgrade_connection(stream).await?;

        Ok(stream)
    }

    async fn accept_peer(&self) -> Result<Connection, Box<dyn Error + Send + Sync>> {
        loop {
            let incoming = self.endpoint.accept().await
                .ok_or("Peer closed the connetion prematurely.")?;

            if incoming.remote_address() == self.peer_addr {
                event!(Level::DEBUG, "Accepting remote...");
                return Ok(incoming.accept()?.await?);
            }
            else {
                event!(Level::DEBUG, "Ignoring remote...");
                incoming.ignore();
            }
        }
    }

    async fn connect_to_peer(&self) -> Result<Connection, Box<dyn Error + Send + Sync>> {
        event!(Level::DEBUG, "Connecting to peer...");
        let conn = self
            .endpoint
            .connect(self.peer_addr, "localhost")?
            .await?;

        event!(Level::DEBUG, "Connected to peer!");

        Ok(conn)
    }

    async fn upgrade_connection(
        &self,
        stream: QuinnStream,
    ) -> Result<PeerConnection, Box<dyn Error + Send + Sync>> {
        let my_keys = utils::ed25519_to_noise(&self.identity.private_key);
        let peer_key = utils::ed25519_verifying_to_x25519(&self.peer_key);

        let transport  =
            NoiseTransportBuilder::<QuinnStream, PeerPacket, PeerPacket>::new(
                my_keys, stream,
            )
            .set_my_type(NoiseSelfType::K)
            .set_peer_type(NoisePeerType::K(peer_key));

        let (transport, _) = match self.role {
            P2pRole::Initiator => transport.build_as_initiator().await?,
            P2pRole::Responder => transport.build_as_responder().await?,
        };

        Ok(transport)
    }
}

pub enum PeerManagerCommand {
    Send(PeerMessageData),
}

#[derive(Debug)]
pub struct PeerManagerHandle {
    pub tx: mpsc::Sender<PeerManagerCommand>,
    task_tracker: TaskTracker,
}

impl PeerManagerHandle {
    pub fn new(
        identity: Myself,
        endpoint: Endpoint,
        peer_key: VerifyingKey,
        peer_addr: SocketAddr,
        token: CancellationToken,
        role: P2pRole,
        tracker: TaskTracker,
        message_consumer: mpsc::Sender<ConnMessage>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(32);

        // Spawns the peer manager actor hypervisor
        tracker.spawn(async move {
            let mut peer_manager = PeerManager {
                identity,
                endpoint,
                peer_key,
                peer_addr,
                token: token.clone(),
                role,
                rx,
                tx: message_consumer,
            };

            loop {
                match peer_manager.run().await {
                    Ok(()) => break,
                    Err(_) => {
                        event!(Level::INFO, "Couldn't connect to the peer. Retrying in 3 seconds.");
                        sleep(Duration::from_secs(3)).await;
                    }
                }
            }
        });

        Self {
            tx,
            task_tracker: tracker.clone(),
        }
    }
}
