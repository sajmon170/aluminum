use std::{
    ffi::OsString,
    io::{self, stdout, Stdout},
    panic::{set_hook, take_hook},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tokio_util::{sync::CancellationToken, task::TaskTracker};

use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        terminal::{
            disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
            LeaveAlternateScreen,
        },
        ExecutableCommand,
    },
    Terminal,
};

use color_eyre::Result;

use clap::Parser;

use crate::controller::AppController;

use libchatty::{
    identity::{Myself, IdentityBuilder, Relay, User, UserDb},
    system::*
};

type Term = Terminal<CrosstermBackend<Stdout>>;

use std::fs::{File, OpenOptions};
use std::io::prelude::*;

use tracing::Level;
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::filter::EnvFilter;

fn init_tui() -> Result<Term> {
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    Ok(terminal)
}

fn restore_tui() -> Result<()> {
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

fn init_panic_hook() {
    let original_hook = take_hook();
    set_hook(Box::new(move |panic_info| {
        // intentionally ignore errors here since we're already in a panic
        let _ = restore_tui();
        original_hook(panic_info);
    }));
}

// TODO: change UserDb::load() to UserDb::import()
/// A peer-to-peer messenger based on out-of-band user identity exchange
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Loads a given database of user identities
    #[arg(long, value_name = "PATH", default_value = get_default_path())]
    db: PathBuf,
    /// Imports another user's identity file
    #[arg(long, value_name = "PATH")]
    import: Option<PathBuf>,
    /// Exports your identity to a file
    #[arg(long, value_name = "PATH")]
    export: Option<PathBuf>,
}

pub struct AppSpawner {
    pub tracker: TaskTracker,
}

fn make_user() -> Result<Myself> {
    println!("Name:");
    let mut name = String::new();
    io::stdin().read_line(&mut name)?;

    println!("Surname:");
    let mut surname = String::new();
    io::stdin().read_line(&mut surname)?;

    println!("Nickname:");
    let mut nickname = String::new();
    io::stdin().read_line(&mut nickname)?;

    println!("Description:");
    let mut description = String::new();
    io::stdin().read_line(&mut description)?;

    let myself = IdentityBuilder::new()
        .name(name.trim().into())
        .surname(surname.trim().into())
        .nickname(nickname.trim().into())
        .description(description.trim().into())
        .build();

    Ok(myself)
}

fn init_tracing(name: &str) -> Result<WorkerGuard> {
    let file = File::create(format!("{name}.log"))?;
    let (non_blocking, guard) = non_blocking(file);

    let env_filter = EnvFilter::builder()
        .with_default_directive(Level::DEBUG.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .init();

    // Alternative subscriber - Tokio Console
    // console_subscriber::init();

    Ok(guard)
}

impl AppSpawner {
    fn start() -> Result<Self> {
        let token = CancellationToken::new();
        let tracker = TaskTracker::new();
        let app_tracker = tracker.clone();
        let args = Args::parse();

        let user_dir = get_user_dir();
        let _ = std::fs::create_dir_all(user_dir);

        let mut db = if args.db.exists() {
            UserDb::load(&args.db)
        }
        else {
            UserDb::new(args.db, make_user()?)
        };

        let name = db.myself.metadata.nickname.trim();
        let _guard = init_tracing(name)?;

        if let Some(path) = args.import {
            let user = User::load_file(&path);
            db.add_user(user);
        }

        if let Some(path) = args.export {
            db.get_user_data().save_file(&path);
            tracker.close();
            return Ok(Self { tracker });
        }

        
        let relay_path = get_relay_path();

        if !relay_path.exists() {
            let mut config = OpenOptions::new()
                .write(true)
                .create_new(true)
                .append(true)
                .open(&relay_path)
                .unwrap();

            writeln!(config, r#"addr = "153.19.219.152:55007""#).unwrap();
            writeln!(config, r#"public_key = "HwPfUAo36nOSDgX13tX1G+ELjoZOK91bL2mmpxu5iYA=""#).unwrap();
        }
        
        let relay = Relay::load(&relay_path)?;

        tracker.spawn(async move {
            init_panic_hook();
            let messages = Vec::<String>::new();
            let mut terminal = init_tui()?;
            let mut app = AppController::new(
                messages,
                &mut terminal,
                app_tracker,
                token,
                Arc::new(Mutex::new(db)),
                relay,
            );
            let _tracing = _guard;
            app.run().await?;
            restore_tui()?;
            Ok::<(), color_eyre::Report>(())
        });

        Ok(Self { tracker })
    }

    pub async fn run() -> Result<()> {
        let handle = AppSpawner::start()?;
        handle.tracker.wait().await;
        Ok(())
    }
}
