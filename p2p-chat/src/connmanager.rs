use libchatty::{
    messaging::{PeerMessageData, RelayRequest, RelayResponse, UserMessage},
    identity::{Myself, Relay},
    noise_session::*,
    quinn_session::*,
    utils,
};

use std::{
    collections::HashMap,
    error::Error,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4}, time::Duration,
};

use crate::peermanager::{P2pRole, PeerManagerCommand, PeerManagerHandle};
use ed25519_dalek::VerifyingKey;
use futures::{sink::SinkExt, stream::StreamExt};
use quinn::{Connection, Endpoint, RecvStream, SendStream};
use rustls::pki_types::CertificateDer;
use tokio::{io::Join, sync::mpsc, time::sleep};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{event, Level};

type RelayConnection<T> = NoiseConnection<T, RelayRequest, RelayResponse>;
type QuicRelayConn = RelayConnection<Join<RecvStream, SendStream>>;

// TODO: Maybe move this to libchatty?
// Try to make this work for both the p2p clients and the relay server
#[derive(Debug)]
struct ConnManager {
    identity: Myself,
    relay: Relay,
    tx: mpsc::Sender<UserMessage>,
    rx: mpsc::Receiver<ConnInstruction>,
    token: CancellationToken,
    tracker: TaskTracker,
    connections: HashMap<VerifyingKey, PeerManagerHandle>,
}

fn make_server_endpoint(
    bind_addr: SocketAddr,
) -> Result<
    (Endpoint, CertificateDer<'static>),
    Box<dyn Error + Send + Sync + 'static>,
> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

impl ConnManager {
    async fn run(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let (endpoint, _conn, mut stream) = self.connect().await?;

        loop {
            tokio::select! {
                Some(ConnInstruction::Send(pubkey, message)) = self.rx.recv() => {
                    if !self.connections.contains_key(&pubkey) {
                        stream.send(RelayRequest::GetUser(pubkey)).await?;
                        
                        let addr = stream.next().await
                            .ok_or("Connection ended unexpectedly")??
                            .as_user_address()
                            .ok_or("Expected address, received something else")?
                            .ok_or("Couldn't find a peer.")?;

                        // ^ TODO: Instead of crashing send a message to the UI
                        // that the peer couldn't be found.

                        event!(Level::INFO, "Trying to connect to: {addr}");
                        self.register_connection(endpoint.clone(), pubkey, addr, P2pRole::Initiator);
                    }

                    self.connections
                        .get(&pubkey)
                        .unwrap()
                        .tx
                        .send(PeerManagerCommand::Send(message))
                        .await?;
                }
                Some(Ok(RelayResponse::AwaitConnection(pubkey, addr))) = stream.next() => {
                    self.register_connection(endpoint.clone(), pubkey, addr, P2pRole::Responder);
                }
                _ = self.token.cancelled() => { break }
                else => { self.token.cancel(); }
            }
        }

        Ok(())
    }

    async fn connect(
        &mut self,
    ) -> Result<
        (Endpoint, Connection, QuicRelayConn),
        Box<dyn Error + Send + Sync>,
    > {
        event!(Level::DEBUG, "Configuring self");
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (mut endpoint, _server_cert) =
            make_server_endpoint(bind_addr).unwrap();

        // TODO: get server address from config
        let server_addr = SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            50007,
        ));

        endpoint.set_default_client_config(
            libchatty::quinn_session::configure_client(),
        );

        event!(Level::DEBUG, "Starting connection");
        let conn = endpoint
            .connect(server_addr, "localhost")?
            .await?;

        event!(Level::DEBUG, "Opened connection");
        let (writer, reader) = conn.open_bi().await?;
        let stream = tokio::io::join(reader, writer);
        event!(Level::DEBUG, "Converted to a stream");
        let mut stream = self.upgrade_relay_connection(stream).await?;
        event!(Level::DEBUG, "Upgraded the connection");

        stream
            .send(RelayRequest::Register(self.identity.get_public_key()))
            .await?;
        let _ack = stream.next().await;

        event!(Level::INFO, "Connected to the server");

        Ok((endpoint, conn, stream))
    }

    fn register_connection(
        &mut self,
        endpoint: Endpoint,
        pubkey: VerifyingKey,
        addr: SocketAddr,
        role: P2pRole,
    ) {
        let handle = PeerManagerHandle::new(
            self.identity.clone(),
            endpoint,
            pubkey,
            addr,
            self.token.clone(),
            role,
            self.tracker.clone(),
            self.tx.clone(),
        );
        self.connections.insert(pubkey, handle);
    }

    async fn upgrade_relay_connection<
        T: Unpin + tokio::io::AsyncRead + tokio::io::AsyncWrite,
    >(
        &self,
        stream: T,
    ) -> Result<RelayConnection<T>, Box<dyn Error + Send + Sync>> {
        let my_keys = utils::ed25519_to_noise(&self.identity.private_key);
        let server_key =
            utils::ed25519_verifying_to_x25519(&self.relay.public_key);

        let (transport, _) =
            NoiseTransportBuilder::<T, RelayRequest, RelayResponse>::new(
                my_keys, stream,
            )
            .set_my_type(NoiseSelfType::I)
            .set_peer_type(NoisePeerType::K(server_key))
            .build_as_initiator()
            .await?;

        Ok(transport)
    }
}

pub enum ConnInstruction {
    Send(VerifyingKey, PeerMessageData),
}

#[derive(Debug)]
pub struct ConnManagerHandle {
    pub tx: mpsc::Sender<ConnInstruction>,
    task_tracker: TaskTracker,
}

impl ConnManagerHandle {
    pub fn new(
        message_tx: mpsc::Sender<UserMessage>,
        identity: Myself,
        relay: Relay,
        tracker: &TaskTracker,
        token: CancellationToken,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);

        let inner_tracker = tracker.clone();
        tracker.spawn(async move {
            let mut conn_manager = ConnManager {
                identity,
                relay,
                tx: message_tx,
                rx: command_rx,
                token: token.clone(),
                tracker: inner_tracker,
                connections: HashMap::new(),
            };

            // Warning! ConnManager keeps its state after a crash!
            // If this causes bugs then create a new ConnManager instance
            // after each crash
            loop {
                match conn_manager.run().await {
                    Ok(()) => break,
                    Err(_) => {
                        event!(Level::INFO, "Couldn't connect to the server. Retrying in 3 seconds.");
                        sleep(Duration::from_secs(3)).await;
                    }
                }
            }
        });
        
        Self {
            tx: command_tx,
            task_tracker: tracker.clone(),
        }
    }
}
