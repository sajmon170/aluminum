mod connmanager;
mod peermanager;
mod eventmanager;
mod message;
mod controller;
mod spawner;
mod tui;
mod component;
mod messageview;
mod friendsview;

use crate::spawner::AppSpawner;
use color_eyre::eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    // TODO - find out which dependency makes this line necessary
    rustls::crypto::ring::default_provider().install_default().unwrap();

    AppSpawner::run().await?;

    Ok(())
}
