#![allow(unused)]

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
use tracing::{debug, info, instrument, trace, Level};
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::filter::EnvFilter;
use std::fs::File;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let _guard = init_tracing()?;
    // TODO - find out which dependency makes this line necessary
    rustls::crypto::ring::default_provider().install_default();

    AppSpawner::run().await?;

    Ok(())
}

// TODO - move this to a common library
// - maybe to libchatty
// - or to another workspace dedicated to generic tooling
fn init_tracing() -> Result<WorkerGuard> {
    let file = File::create("tracing.log")?;
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

