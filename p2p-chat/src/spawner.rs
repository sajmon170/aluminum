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

use clap::Parser;

use crate::controller::AppController;

use libchatty::identity::{Myself, User, UserDb, Relay};

type Term = Terminal<CrosstermBackend<Stdout>>;

fn init_tui() -> io::Result<Term> {
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    Ok(terminal)
}

fn restore_tui() -> io::Result<()> {
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

/// A peer-to-peer messenger based on out-of-band user identity exchange
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Loads a given database of user identities
    #[arg(long, value_name = "PATH", default_value = get_default_path())]
    db: PathBuf,
    /// Loads another user's identity file
    #[arg(long, value_name = "PATH")]
    load: Option<PathBuf>,
    /// Exports your identity to a file
    #[arg(long, value_name = "PATH")]
    export: Option<PathBuf>
}

fn get_default_path() -> OsString {
    dirs::data_dir()
        .unwrap()
        .join("aluminum")
        .join("user.db")
        .into_os_string()
}

fn get_relay_path() -> PathBuf {
    dirs::data_dir()
        .unwrap()
        .join("aluminum")
        .join("relay.toml")
}

pub struct AppSpawner {
    pub tracker: TaskTracker,
}

impl AppSpawner {
    // TODO: Fix the bug where the app panics if ~/.local/share/aluminum doesn't exist
    // TODO: separate database handling into another function
    fn start() -> io::Result<Self> {
        let token = CancellationToken::new();
        let tracker = TaskTracker::new();
        let app_tracker = tracker.clone();
        let args = Args::parse();

        let mut db = if args.db.exists() {
            UserDb::load(&args.db)
        } else {
            UserDb::new(
                args.db,
                Myself::new(
                    "Szymon",
                    "Zadworny",
                    "sajmon",
                    "kurwa nie zdam xd",
                ),
            )
        };

        if let Some(path) = args.load {
            let user = User::load_file(&path);
            db.add_user(user);
        }

        if let Some(path) = args.export {
            db.get_user_data().save_file(&path);
            tracker.close();
            return Ok(Self { tracker });
        }

        let relay = Relay::load(&get_relay_path())?;

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
                relay
            );
            app.run().await?;
            restore_tui()?;
            Ok::<(), io::Error>(())
        });

        Ok(Self { tracker })
    }

    pub async fn run() -> io::Result<()> {
        let handle = AppSpawner::start()?;
        handle.tracker.wait().await;
        Ok(())
    }
}
