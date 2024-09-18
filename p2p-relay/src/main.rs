#![allow(unused)]
use futures::{sink::SinkExt, stream::StreamExt};

use libchatty::{
    identity::{Myself, UserDb},
    messaging::{RelayRequest, RelayResponse},
    noise_session::*,
    quinn_session::*,
    utils,
};

use std::{
    error::Error,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use clap::Parser;

use base64::prelude::*;

use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

// TODO - move this into a sseparate library
use tracing::{debug, event, info, instrument, trace, Level};
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::filter::EnvFilter;

// TODO - move this into a sseparate library
use color_eyre::eyre::Result;

use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use std::fs::File;

// TODO: allow exporting your identity as Relay
// -> You need to provide an IP address
// ...or maybe ignore this and just write the relay file manually.
// The relay doesn't know its public IP address and port, only its public key
// => Add a get public key flag

// TODO - fix connmanager (currently it uses db instead of the Relay struct)

use tokio::io::{AsyncRead, AsyncWrite, Join};

use tokio::sync::mpsc;

use quinn::{Connection, RecvStream, SendStream};

pub fn make_server_endpoint(
    bind_addr: SocketAddr,
) -> Result<
    (Endpoint, CertificateDer<'static>),
    Box<dyn Error + Send + Sync + 'static>,
> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

enum Notify {
    Call(SocketAddr),
}

type ConnectionDb = HashMap<VerifyingKey, SocketAddr>;
type NotifyDb = HashMap<SocketAddr, mpsc::Sender<Notify>>;

async fn process(
    conn: Connection,
    db: Arc<Mutex<UserDb>>,
    conn_db: Arc<Mutex<ConnectionDb>>,
    notify_db: Arc<Mutex<NotifyDb>>,
    mut notify_rx: mpsc::Receiver<Notify>
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let addr = conn.remote_address();

    let (writer, reader) = conn.accept_bi().await.unwrap();
    let stream = tokio::io::join(reader, writer);

    let keys = {
        let mut lock = db.lock().unwrap();
        let key = lock.get_master_key();
        utils::ed25519_to_noise(key)
    };

    let mut stream = NoiseTransportBuilder::<
        Join<RecvStream, SendStream>,
        RelayRequest,
        RelayResponse,
    >::new(keys, stream)
    .set_my_type(NoiseSelfType::K)
    .set_peer_type(NoisePeerType::I)
    .build_as_responder()
    .await
    .expect("Handshake error");

    let (mut tx, mut rx) = stream.split();

    loop {
        tokio::select! {
            Some(Ok(msg)) = rx.next() => {
                match msg {
                    RelayRequest::Register(pubkey) => {
                        event!(Level::INFO, "Registered a new user.");
                        let _guard = {
                            let mut db = conn_db.lock().unwrap();
                            db.insert(pubkey, addr);
                        };

                        tx.send(RelayResponse::Ack).await;
                    }
                    RelayRequest::GetUser(pubkey) => {
                        let result = {
                            let db = conn_db.lock().unwrap();
                            db.get(&pubkey).and_then(|addr| Some(addr.clone()))
                        };

                        let db = notify_db.clone();

                        tokio::join!(
                            tx.send(RelayResponse::UserAddress(result.clone())),
                            async move {
                                let tx = {
                                    let mut db = db.lock().unwrap();
                                    db.get(&result.unwrap()).unwrap().clone()
                                };
                                tx.send(Notify::Call((addr.clone()))).await;
                            }
                        );
                    }
                    RelayRequest::Ack => {}
                    RelayRequest::Bye => break,
                }
            }
            Some(notification) = notify_rx.recv() => {
                let Notify::Call(addr) = notification;
                let key = {
                    let db = conn_db.lock().unwrap();
                    db.iter().filter(|(k, v)| **v == addr).next().unwrap().0.clone()
                };
                tx.send(RelayResponse::AwaitConnection(key, addr)).await?;
            }
        }
        /*
        while let Some(Ok(msg)) = rx.next().await {
            match msg {
                RelayRequest::Register(pubkey) => {
                    event!(Level::INFO, "Registered a new user.");
                    let _guard = {
                        let mut db = conn_db.lock().unwrap();
                        db.insert(pubkey, addr);
                    };

                    tx.send(RelayResponse::Ack).await;
                }
                RelayRequest::GetUser(pubkey) => {
                    let result = {
                        let db = conn_db.lock().unwrap();
                        db.get(&pubkey).and_then(|addr| Some(addr.clone()))
                    };

                    //tokio::join
                    // TODO - notify the user
                    tx.send(RelayResponse::UserAddress(result)).await;
                }
                RelayRequest::Ack => {}
                RelayRequest::Bye => break,
            }
        }
*/
    }

    Ok(())
}

fn get_default_path() -> PathBuf {
    // TODO: change app's name
    dirs::data_dir()
        .unwrap()
        .join("aluminum")
        .join("newserver.db")
}

/// Aluminum relay server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Prints your identity to stdout
    #[arg(long, value_name = "PATH")]
    public: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    color_eyre::install()?;
    let _guard = init_tracing()?;

    let path = get_default_path();
    let serverdb = if path.exists() {
        UserDb::load(&path)
    } else {
        UserDb::new(
            path,
            Myself::new(
                "Serwer",
                "Serwerowsky",
                "server",
                "serwuje użytkowników z tradycją od 2024 roku",
            ),
        )
    };

    let conndb = Arc::new(Mutex::new(ConnectionDb::new()));
    let notifydb = Arc::new(Mutex::new(NotifyDb::new()));

    let args = Args::parse();
    if args.public {
        let public = serverdb.myself.get_public_key();
        println!("{}", BASE64_STANDARD.encode(public.as_bytes()));
        return Ok(());
    }

    rustls::crypto::ring::default_provider().install_default();

    let serverdb = Arc::new(Mutex::new(serverdb));

    let addr: SocketAddr = "127.0.0.1:50007".parse().unwrap();
    let (endpoint, _server_cert) = make_server_endpoint(addr).unwrap();

    loop {
        let conn = endpoint.accept().await.unwrap().await.unwrap();
        let addr = conn.remote_address();
        let (tx, rx) = mpsc::channel(32);
        let _guard = {
            let mut db = notifydb.lock().unwrap();
            db.insert(addr, tx);
        };
        event!(
            Level::INFO,
            "Opened a new connection from {}",
            conn.remote_address()
        );
        tokio::spawn(process(
            conn,
            serverdb.clone(),
            conndb.clone(),
            notifydb.clone(),
            rx
        ));
    }
}

fn init_tracing() -> Result<WorkerGuard> {
    let file = File::create("server.log")?;
    let (non_blocking, guard) = non_blocking(file);

    let env_filter = EnvFilter::builder()
        .with_default_directive(Level::DEBUG.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .init();

    Ok(guard)
}
