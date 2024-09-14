use libchatty::{
    identity::UserDb,
    messaging::{RelayRequest, RelayResponse},
    noise_session::*,
    quinn_session::*,
    utils,
};

use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{Arc, Mutex},
};

use crate::message::DisplayMessage;
use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use futures::{sink::SinkExt, stream::StreamExt};
use libchatty::identity::{Myself, Relay};
use quinn::{ClientConfig, Endpoint};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{event, span, Level};

type RelayConnection<T> = NoiseConnection<T, RelayRequest, RelayResponse>;

// TODO: Maybe move this to libchatty?
// Try to make this work both for the p2p clients and the relay server
// TODO: Maybe rename this, this acts like a Main
#[derive(Debug)]
struct ConnManager {
    identity: Myself,
    relay: Relay,
    sender: mpsc::Sender<DisplayMessage>,
    rx: mpsc::Receiver<ConnInstruction>,
    token: CancellationToken, // TODO:
                              //incoming_tx: mpsc::Sender<DisplayMessage>,
                              //egress_rx: mpsc::Receiver<DisplayMessage>
}

impl ConnManager {
    fn new(
        identity: Myself,
        relay: Relay,
        sender: mpsc::Sender<DisplayMessage>,
        rx: mpsc::Receiver<ConnInstruction>,
        token: CancellationToken,
    ) -> ConnManager {
        ConnManager {
            identity,
            relay,
            sender,
            rx,
            token,
        }
    }

    // This function currently handles incoming messages - it sends them through
    // an mpsc::Sender to the local receiver.
    // It doesn't handle sending egress messages to the remote receiver!!!
    async fn run(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        //let listener = TcpListener::bind("127.0.0.1:50007").await?;
        //let stream = TcpStream::connect(&self.relay.addr).await?;

        event!(Level::INFO, "Configuring self");
        let mut endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap())?;
        let server_addr = SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            50007,
        ));
        endpoint.set_default_client_config(
            libchatty::quinn_session::configure_client(),
        );

        event!(Level::INFO, "Starting connection");
        //let conn = endpoint.accept().await.unwrap().await.unwrap();
        let conn = endpoint
            .connect(server_addr, "localhost")
            .unwrap()
            .await
            .unwrap();

        event!(Level::INFO, "Opened connection");
        let (writer, reader) = conn.open_bi().await.unwrap();
        let stream = tokio::io::join(reader, writer);
        event!(Level::INFO, "Converted to a stream");
        let mut stream = self.upgrade_relay_connection(stream).await?;
        event!(Level::INFO, "Upgraded the connection");

        //1. Connect to relay
        //2. Await messages from relay
        //3. Make new connection on a received message
        //Temp: treat the relay as the end server

        // Warning: This method will spawn more tasks for each connection!

        // Problem - ConnManager concerns itself with the UI rendering
        // - it should only forward messages

        loop {
            /*
                        tokio::select! {
                            Some(Ok(msg)) = stream.next() => {
                                if let Message::Send(msg) = msg {
                                    let msg = DisplayMessage {
                                        content: msg,
                                        author: String::from("Other"),
                                        timestamp: Utc::now()
                                    };
                                    self.sender.send(msg).await;
                                }
                            }
                            Some(ConnInstruction::Send(msg)) = self.rx.recv() => {
                                stream.send(Message::Send(msg)).await?;
                            }
                            _ = self.token.cancelled() => { break; }
                            else => { self.token.cancel(); }
                        }
            */

            tokio::select! {
                // Problem: how do we store incoming messages from different users?
                // Solution:
                // the user appends the receiver key to each message
                // the connection manager appends the public key of each friend
                // => This might imply allocations on each send but each send happens
                // rarely
                // => Incoming messages aren't that often
                // We avoid having to read arc mutex db

                // Alternative: write incoming message to the db under a specific record
                // Notify user that the database has been modified on that specific record
                // Give each user an ID
                // Create a hashmap of User ID -> List<Message> which contains a conversation log

                // That database would have to store not only the message, but also
                // its metadata: creation time, user ID (to prevent ambiguity when
                // the same person sends multiple messages in succession)
                // Warning - we can't share a reference to the db when drawing
                // since each draw call takes too much time - we need to copy data
                // on each draw call.

                // Or - we need to implement a UI-side mirror of the database
                // that would have to be kept in sync

                // ----------------------------------
                // Opening Connections:
                // Connections are opened only by the ConnManager.
                // The app controller doesn't open them by itself - it only wants
                // a handle to an actual connection with user.

                Some(ConnInstruction::GetUser(pubkey)) = self.rx.recv() => {
                    stream.send(RelayRequest::GetUser(pubkey)).await?;
                    let resp = stream.next().await.unwrap().unwrap();
                    if let RelayResponse::UserAddress(addr) = resp {
                        event!(Level::INFO, "addr: {addr}");
                    }

                    //^ This needs to be done through RPCs. Currently the caller
                    // needs to care about the return type (if let GenericEnum = ...)
                    // Unacceptable!
                }
                _ = self.token.cancelled() => { break; }
                else => { self.token.cancel(); }
            }
        }

        Ok(())
    }

    async fn handle_incoming() {
        //let listener = UtpListener::bind("127.0.0.1:0").await?;
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

        let transport =
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
    GetUser(VerifyingKey),
    Send(String),
}

#[derive(Debug)]
pub struct ConnManagerHandle {
    pub tx: mpsc::Sender<ConnInstruction>,
    task_tracker: TaskTracker,
}

impl ConnManagerHandle {
    pub fn new(
        identity: Myself,
        relay: Relay,
        sender: mpsc::Sender<DisplayMessage>,
        tracker: &TaskTracker,
        token: CancellationToken,
    ) -> Self {
        let (tx, rx) = mpsc::channel(32);
        let mut conn_manager =
            ConnManager::new(identity, relay, sender, rx, token);

        tracker.spawn(async move { conn_manager.run().await.unwrap() });

        Self {
            tx,
            task_tracker: tracker.clone(),
        }
    }
}
