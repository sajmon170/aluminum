use libchatty::{
    identity::UserDb, messaging::RelayMessage, noise_session::*, utils,
};

use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use tokio::{net::TcpStream, sync::mpsc};

use tokio_util::{sync::CancellationToken, task::TaskTracker};

use futures::{sink::SinkExt, stream::StreamExt};

use crate::message::DisplayMessage;

use chrono::Utc;

use libchatty::identity::{Myself, Relay};

// TODO: Rename NoiseStream into NoiseConnection, NoiseConn or NoiseFramed
// Rename MessageStream
type RelayConnection = NoiseStream<TcpStream, RelayMessage>;

// TODO: Maybe move this to libchatty?
// Try to make this work both for the p2p clients and the relay server
// TODO: Maybe rename this, this acts like a Main
#[derive(Debug)]
struct ConnManager {
    identity: Myself,
    relay: Relay,
    sender: mpsc::Sender<DisplayMessage>,
    relay_rx: mpsc::Receiver<RelayMessage>,
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
        let stream = TcpStream::connect(&self.relay.addr).await?;
        let mut stream = self.upgrade_relay_connection(stream).await?;
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
                
                Some(RelayMessage::GetUser(pubkey)) = self.relay_rx.recv() => {
                    stream.send(Message::Send(msg)).await?;
                }
                _ = self.token.cancelled() => { break; }
                else => { self.token.cancel(); }
            }
        }

        Ok(())
    }

    async fn upgrade_relay_connection(
        &self,
        stream: TcpStream,
    ) -> Result<RelayConnection, Box<dyn Error + Send + Sync>> {
        let my_keys = utils::ed25519_to_noise(&self.identity.private_key);
        let server_key = utils::ed25519_verifying_to_x25519(&self.relay.public_key);

        let transport =
            NoiseTransportBuilder::<TcpStream, RelayMessage>::new(my_keys, stream)
                .set_my_type(NoiseSelfType::I)
                .set_peer_type(NoisePeerType::K(server_key))
                .build_as_initiator()
                .await?;

        Ok(transport)
    }
}

pub enum ConnInstruction {
    
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
        let mut conn_manager = ConnManager::new(identity, relay, sender, rx, token);

        tracker.spawn(async move { conn_manager.run().await });

        Self {
            tx,
            task_tracker: tracker.clone(),
        }
    }
}
