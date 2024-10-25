mod component;
mod action;
mod connmanager;
mod controller;
mod eventmanager;
mod friendsview;
mod message;
mod messagerepl;
mod messageview;
mod peermanager;
mod spawner;
mod tui;

use crate::spawner::AppSpawner;
use color_eyre::eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    // TODO - find out which dependency makes this line necessary
    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap();

    AppSpawner::run().await?;

    Ok(())
}
