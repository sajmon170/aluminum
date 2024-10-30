use libchatty::{
    identity::Myself,
    messaging::{PeerMessageData, PeerPacket, UserMessage},
    noise_session::*,
    noise_transport::*,
    system::{self, FileHandle, FileMetadata},
    utils,
};

use std::{error::Error, net::SocketAddr, path::PathBuf, time::Duration};

use ed25519_dalek::VerifyingKey;
use futures::{sink::SinkExt, stream::StreamExt};
use quinn::{Connection, Endpoint};

use tokio::{
    fs::{File, OpenOptions},
    io::AsyncReadExt,
    sync::mpsc,
    time::sleep,
};

use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{event, Level};

use crate::connmanager::ConnMessage;

type QuinnStream = tokio::io::Join<quinn::RecvStream, quinn::SendStream>;
type PeerConnection = NoiseTransport<QuinnStream, PeerPacket, PeerPacket>;

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
    rx: mpsc::Receiver<PeerCommand>,
    tx: mpsc::Sender<ConnMessage>,
    conn: Option<PeerConnection>,
    // TODO - replace this with a database of invites
    sent_invite: Option<FileHandle>,
    recv_invite: Option<FileMetadata>,
}

impl PeerManager {
    async fn run(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        event!(Level::DEBUG, "Trying to hole-punch...");
        self.connect().await?;

        loop {
            tokio::select! {
                Some(Ok(packet)) = self.conn.as_mut().unwrap().next() => {
                    self.handle_incoming_packet(packet).await?
                }
                Some(command) = self.rx.recv() => {
                    self.handle_egress_command(command).await?
                }
                _ = self.token.cancelled() => { break; }
                else => { self.token.cancel() }
            }
        }

        Ok(())
    }

    async fn handle_egress_command(
        &mut self,
        command: PeerCommand,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match command {
            PeerCommand::Send(msg) => self.send_message(msg).await?,
            PeerCommand::ShareFile(file) => self.send_file_invite(file).await?,
            PeerCommand::GetFile => self.download_file().await?,
        }

        Ok(())
    }

    async fn handle_incoming_packet(
        &mut self,
        packet: PeerPacket,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match packet {
            PeerPacket::Send(msg) => self.receive_message(msg).await?,
            PeerPacket::Share(file) => self.receive_file_invite(file).await?,
            PeerPacket::GimmeFile => self.upload_file().await?,
            _ => (),
        }

        Ok(())
    }

    async fn send_packet(
        &mut self,
        msg: PeerPacket,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.conn
            .as_mut()
            .ok_or("Can't send message: not connected to peer.")?
            .send(msg)
            .await?;

        Ok(())
    }

    async fn send_message(
        &mut self,
        msg: PeerMessageData,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let text = match &msg {
            PeerMessageData::Text(text) => text.clone(),
        };
        event!(Level::INFO, "Sending message: {text}");
        self.send_packet(PeerPacket::Send(msg)).await?;

        Ok(())
    }

    async fn receive_message(
        &mut self,
        msg: PeerMessageData,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let text = match &msg {
            PeerMessageData::Text(text) => text.clone(),
        };

        event!(Level::INFO, "Received message: {text}");

        self.tx
            .send(ConnMessage::UserMessage(UserMessage::new(
                self.peer_key,
                msg,
            )))
            .await?;

        Ok(())
    }

    async fn send_file_invite(
        &mut self,
        path: PathBuf,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.sent_invite = Some(FileHandle::new(path).await?);
        let meta = self.sent_invite.as_ref().unwrap().get_metadata();
        self.send_packet(PeerPacket::Share(meta.clone())).await?;

        Ok(())
    }

    async fn receive_file_invite(
        &mut self,
        file: FileMetadata,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.recv_invite = Some(file.clone());

        self.tx.send(ConnMessage::FileInvite(file)).await?;

        Ok(())
    }

    async fn upload_file(
        &mut self,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.sent_invite.as_ref().unwrap().get_path();

        let mut file = File::open(path).await?;
        let mut socket = self.conn.as_mut().unwrap().get_mut();

        event!(Level::INFO, "Beginning file upload");
        tokio::io::copy(&mut file, &mut socket).await?;
        event!(Level::INFO, "Finished uploading");

        Ok(())
    }

    async fn download_file(
        &mut self,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let invite = self
            .recv_invite
            .as_ref()
            .ok_or("Can't download without a matching invite.")?
            .clone();

        let save_path = system::get_downloads_dir().join(&invite.name);

        event!(Level::DEBUG, "Preparing for download, saving file @ {:?}", save_path);
        
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(save_path)
            .await?;

        self.send_packet(PeerPacket::GimmeFile).await?;

        let mut socket =
            self.conn.as_mut().unwrap().get_mut().take(invite.size);

        event!(Level::INFO, "Beginning file download");
        tokio::io::copy(&mut socket, &mut file).await?;
        event!(Level::INFO, "Finished downloading");

        self.tx.send(ConnMessage::DownloadedFile).await?;

        Ok(())
    }

    async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let (writer, reader) = {
            let (incoming, outgoing) =
                tokio::join!(self.accept_peer(), self.connect_to_peer());

            event!(Level::DEBUG, "Hole punch success");

            match self.role {
                P2pRole::Initiator => outgoing?.open_bi().await?,
                P2pRole::Responder => incoming?.accept_bi().await?,
            }
        };

        let stream = tokio::io::join(reader, writer);
        let stream = self.upgrade_connection(stream).await?;
        self.conn = Some(stream);

        Ok(())
    }

    async fn accept_peer(
        &self,
    ) -> Result<Connection, Box<dyn Error + Send + Sync>> {
        loop {
            let incoming = self
                .endpoint
                .accept()
                .await
                .ok_or("Peer closed the connetion prematurely.")?;

            if incoming.remote_address() == self.peer_addr {
                event!(Level::DEBUG, "Accepting remote...");
                return Ok(incoming.accept()?.await?);
            } else {
                event!(Level::DEBUG, "Ignoring remote...");
                incoming.ignore();
            }
        }
    }

    async fn connect_to_peer(
        &self,
    ) -> Result<Connection, Box<dyn Error + Send + Sync>> {
        event!(Level::DEBUG, "Connecting to peer...");
        let conn = self.endpoint.connect(self.peer_addr, "localhost")?.await?;

        event!(Level::DEBUG, "Connected to peer!");

        Ok(conn)
    }

    async fn upgrade_connection(
        &self,
        stream: QuinnStream,
    ) -> Result<PeerConnection, Box<dyn Error + Send + Sync>> {
        let my_keys = utils::ed25519_to_noise(&self.identity.private_key);
        let peer_key = utils::ed25519_verifying_to_x25519(&self.peer_key);

        let stream = NoiseBuilder::<QuinnStream>::new(my_keys, stream)
            .set_my_type(NoiseSelfType::K)
            .set_peer_type(NoisePeerType::K(peer_key));

        let stream = match self.role {
            P2pRole::Initiator => stream.build_as_initiator().await?,
            P2pRole::Responder => stream.build_as_responder().await?,
        };

        let transport =
            NoiseTransport::<QuinnStream, PeerPacket, PeerPacket>::new(stream);

        Ok(transport)
    }
}

pub enum PeerCommand {
    Send(PeerMessageData),
    ShareFile(PathBuf),
    GetFile,
}

#[derive(Debug)]
pub struct PeerManagerHandle {
    pub tx: mpsc::Sender<PeerCommand>,
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
                conn: None,
                sent_invite: None,
                recv_invite: None
            };

            loop {
                match peer_manager.run().await {
                    Ok(()) => break,
                    Err(e) => {
                        event!(Level::INFO, "Couldn't connect to the peer. Retrying in 3 seconds.");
                        event!(Level::DEBUG, "Error: {}", e);
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
