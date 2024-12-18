use libchatty::{
    messaging::{PeerMessageData, RelayRequest, RelayResponse, UserMessage},
    identity::{Myself, Relay, UserDb},
    noise_session::*,
    quinn_session::*,
    noise_transport::*,
    system::FileMetadata,
    utils,
};

use std::{
    collections::HashMap,
    error::Error,
    net::SocketAddr, time::Duration,
    sync::{Arc, Mutex}
};

use crate::peermanager::{P2pRole, PeerCommand, PeerManagerHandle};
use ed25519_dalek::VerifyingKey;
use futures::{sink::SinkExt, stream::StreamExt};
use quinn::{Connection, Endpoint, RecvStream, SendStream};
use rustls::pki_types::CertificateDer;
use tokio::{
    io::{Join, AsyncRead, AsyncWrite},
    sync::mpsc, time::sleep
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{event, Level};

type RelayConnection<T> = NoiseTransport<T, RelayRequest, RelayResponse>;
type QuicRelayConn = RelayConnection<Join<RecvStream, SendStream>>;

// TODO: Maybe move this to libchatty?
// Try to make this work for both the p2p clients and the relay server
#[derive(Debug)]
struct ConnManager {
    identity: Myself,
    relay: Relay,
    tx: mpsc::Sender<ConnMessage>,
    rx: mpsc::Receiver<ConnCommand>,
    token: CancellationToken,
    tracker: TaskTracker,
    connections: HashMap<VerifyingKey, PeerManagerHandle>,
    db: Arc<Mutex<UserDb>>
}

pub enum ConnMessage {
    UserMessage(UserMessage),
    // TODO - change this to DownloadedFile(Hash)
    DownloadedFile,
    ServerOffline,
    Connecting,
    Connected
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
                Some(ConnCommand {to, command}) = self.rx.recv() => {
                    if !self.connections.contains_key(&to) {
                        stream.send(RelayRequest::GetUser(to)).await?;
                        
                        let addr = stream.next().await
                            .ok_or("Connection ended unexpectedly")??
                            .as_user_address()
                            .ok_or("Expected address, received something else")?
                            .ok_or("Couldn't find a peer.")?;

                        // ^ TODO: Instead of crashing send a message to the UI
                        // that the peer couldn't be found.

                        event!(Level::INFO, "Trying to connect to: {addr}");
                        self.register_connection(endpoint.clone(), to, addr, P2pRole::Initiator);
                    }

                    self.connections
                        .get(&to)
                        .unwrap()
                        .tx
                        .send(command)
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
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let (mut endpoint, _server_cert) =
            make_server_endpoint(bind_addr).unwrap();

        let server_addr = self.relay.addr;

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
        let _ = self.tx.send(ConnMessage::Connected).await;

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
            self.db.clone()
        );
        self.connections.insert(pubkey, handle);
    }

    async fn upgrade_relay_connection<T: Unpin + AsyncRead + AsyncWrite>(
        &self,
        stream: T,
    ) -> Result<RelayConnection<T>, Box<dyn Error + Send + Sync>> {
        let my_keys = utils::ed25519_to_noise(&self.identity.private_key);
        let server_key =
            utils::ed25519_verifying_to_x25519(&self.relay.public_key);

        let stream =
            NoiseBuilder::new(my_keys, stream)
            .set_my_type(NoiseSelfType::I)
            .set_peer_type(NoisePeerType::K(server_key))
            .build_as_initiator()
            .await?;

        let transport = NoiseTransport::<T, RelayRequest, RelayResponse>::new(stream);

        Ok(transport)
    }
}

struct ConnCommand {
    to: VerifyingKey,
    command: PeerCommand
}

#[derive(Debug)]
pub struct ConnManagerHandle {
    tx: mpsc::Sender<ConnCommand>,
    task_tracker: TaskTracker,
}

impl ConnManagerHandle {
    pub fn new(
        message_tx: mpsc::Sender<ConnMessage>,
        identity: Myself,
        relay: Relay,
        tracker: &TaskTracker,
        token: CancellationToken,
        db: Arc<Mutex<UserDb>>
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);

        let inner_tracker = tracker.clone();
        tracker.spawn(async move {
            let mut conn_manager = ConnManager {
                identity,
                relay,
                tx: message_tx.clone(),
                rx: command_rx,
                token: token.clone(),
                tracker: inner_tracker,
                connections: HashMap::new(),
                db
            };

            // Warning! ConnManager keeps its state after a crash!
            // If this causes bugs then create a new ConnManager instance
            // after each crash
            loop {
                tokio::select! {
                    Err(_) = conn_manager.run() => {
                        event!(Level::INFO, "Couldn't connect to the server. Retrying in 3 seconds.");
                        let _ = message_tx.send(ConnMessage::ServerOffline).await;
                        sleep(Duration::from_secs(3)).await;
                        let _ = message_tx.send(ConnMessage::Connecting).await;
                    }
                    _ = token.cancelled() => { break }
                    else => { token.cancel() }
                }
            }
        });
        
        Self {
            tx: command_tx,
            task_tracker: tracker.clone(),
        }
    }

    pub async fn send(&mut self, to: VerifyingKey, command: PeerCommand) {
        let _ = self.tx.send(ConnCommand { to, command }).await;
    }
}
