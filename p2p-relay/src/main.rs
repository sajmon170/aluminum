#![allow(unused)]
use tokio::net::{TcpListener, TcpStream};

use futures::{sink::SinkExt, stream::StreamExt};

use libchatty::{
    identity::{Myself, UserDb}, messaging::Message, noise_session::*, utils
};

use std::{
    error::Error,
    sync::{Arc, Mutex},
    path::PathBuf
};

use clap::Parser;

use base64::prelude::*;

// TODO: allow exporting your identity as Relay
// -> You need to provide an IP address
// ...or maybe ignore this and just write the relay file manually.
// The relay doesn't know its public IP address and port, only its public key
// => Add a get public key flag

// TODO - fix connmanager (currently it uses db instead of the Relay struct)

async fn process(stream: TcpStream, db: Arc<Mutex<UserDb>>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let keys = {
        let mut lock = db.lock().unwrap();
        let key = lock.get_master_key();
        utils::ed25519_to_noise(key)
    };
    
    let mut stream = NoiseTransportBuilder::<TcpStream, Message>::new(keys, stream)
        .set_my_type(NoiseSelfType::K)
        .set_peer_type(NoisePeerType::I)
        .build_as_responder().await?;

    let (mut tx, mut rx) = stream.split();

    let mut timeout = tokio::time::interval(tokio::time::Duration::from_secs(2));

    let _ = tokio::join!(
        async move {
            timeout.tick().await;
            tx.send(Message::Ack).await?;
            println!("Sent.");
            timeout.tick().await;
            tx.send(Message::Send(String::from("Jazda z kurwami"))).await?;
            println!("Sent.");
            timeout.tick().await;
            tx.send(Message::Send(String::from("Żółć, polskie znaki"))).await?;
            println!("Sent.");
            timeout.tick().await;
            tx.send(Message::Send("お前はもう死んでいる".into())).await?;
            println!("Sent.");
            timeout.tick().await;
            tx.send(Message::Bye).await?;
            println!("Sent.");

            Ok::<(), std::io::Error>(())
        },

        async move {
            while let Some(Ok(msg)) = rx.next().await {
                match msg {
                    Message::Send(msg) => { println!("Received message: [{msg}]") }
                    Message::Bye => {
                        println!("Finished session. Disconnecting...");
                        break
                    }
                    _ => { println!{"Not implemented"} }
                }
            }
        }
    );

    println!("Connection closed.");
    Ok(())
}

fn get_default_path() -> PathBuf {
    // TODO: change app's name
    dirs::data_dir().unwrap()
        .join("aluminum")
        .join("newserver.db")
}

/// Aluminum relay server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Prints your identity to stdout
    #[arg(long, value_name = "PATH")]
    public: bool
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let path = get_default_path();
    let serverdb = if path.exists() {
        UserDb::load(&path)
    }
    else {
        UserDb::new(
            path,
            Myself::new(
                "Serwer",
                "Serwerowsky",
                "server",
                "serwuje więcej użytkowników niż twoja stara"
            )
        )
    };

    let args = Args::parse();
    if args.public {
        let public = serverdb.myself.get_public_key();
        println!("{}", BASE64_STANDARD.encode(public.as_bytes()));
        return Ok(());
    }

    let serverdb = Arc::new(Mutex::new(serverdb));
    
    let listener = TcpListener::bind("127.0.0.1:50007").await?;

    loop {
        let (mut stream, _) = listener.accept().await?;
        println!("New connection");
        tokio::spawn(process(stream, serverdb.clone()));
    }
}
