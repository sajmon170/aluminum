use std::{
    io::{self, Stdout},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ed25519_dalek::VerifyingKey;
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::{fs::File, sync::mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use image::{DynamicImage, ImageError, ImageReader};
use ratatui_image::picker::Picker;

use crate::{
    action::AppAction,
    connmanager::ConnManagerHandle,
    eventmanager::{AppEvent, EventManagerHandle},
    messagerepl::{Cli, Command, Parser},
    messageview::MessageViewAction,
    peermanager::PeerCommand,
    tui::{Tui, TuiAction},
};

use libchatty::{
    identity::{Myself, Relay, UserDb},
    messaging::{PeerMessageData, UserMessage},
    system::{FileHandle, FileMetadata},
};

use color_eyre::Result;
use tracing::{event, Level};

type Term = Terminal<CrosstermBackend<Stdout>>;

pub struct AppController<'a> {
    terminal: &'a mut Term,
    tui: Tui<'a>,
    event_manager: EventManagerHandle,
    conn_manager: ConnManagerHandle,
    tracker: TaskTracker,
    token: CancellationToken,
    db: Arc<Mutex<UserDb>>,
    pending_download: Option<FileMetadata>,
}

impl<'a> AppController<'a> {
    pub fn new(
        msgs: Vec<String>,
        terminal: &'a mut Term,
        tracker: TaskTracker,
        token: CancellationToken,
        db: Arc<Mutex<UserDb>>,
        relay: Relay,
    ) -> Self {
        let mut picker = Picker::from_termios().unwrap();
        picker.guess_protocol();
        let tui = Tui::new(db.clone(), picker);

        let (message_tx, message_rx) = mpsc::channel(32);
        let event_manager =
            EventManagerHandle::new(message_rx, &tracker, token.clone());

        let identity: Myself;
        {
            let db = db.lock().unwrap();
            identity = db.myself.clone();
        }

        let conn_manager = ConnManagerHandle::new(
            message_tx,
            identity,
            relay,
            &tracker,
            token.clone(),
            db.clone(),
        );

        Self {
            terminal,
            tui,
            event_manager,
            conn_manager,
            tracker,
            token,
            db,
            pending_download: None,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                Some(event) = self.event_manager.event_rx.recv() => {
                    if let Some(mut action) = self.handle_event(event) {
                        while let Some(next_action) = self.execute(action).await? {
                            action = next_action;
                        }

                    }

                },
                _ = self.token.cancelled() => { break; },
                else => { self.token.cancel() }
            }
        }

        self.tracker.close();

        Ok(())
    }

    fn handle_event(&mut self, event: AppEvent) -> Option<AppAction> {
        match event {
            AppEvent::FrameTick => Some(AppAction::Redraw),
            AppEvent::KeyPress(key) => self.tui.handle_kbd_event(key),
            AppEvent::ReceiveMessage(msg) => {
                Some(AppAction::ReceiveMessage(msg))
            }
            AppEvent::NotifyDownloaded => {
                Some(AppAction::ReceiveDownloadedFile)
            }
            AppEvent::SetConnected => Some(AppAction::SetConnected),
            AppEvent::SetConnecting => Some(AppAction::SetConnecting),
            AppEvent::SetOffline => Some(AppAction::SetOffline),
        }
    }

    fn receive_message(&mut self, msg: UserMessage) -> Option<AppAction> {
        self.tui.add_user_message(msg.author, &msg);
        self.add_user_message(msg.author, msg.clone());

        match msg.content {
            PeerMessageData::FileMeta(meta) => self.receive_invite(meta),
            _ => None,
        }
    }

    fn receive_invite(&mut self, invite: FileMetadata) -> Option<AppAction> {
        self.pending_download = Some(invite);

        if let Some(t) = &self.pending_download.as_ref().unwrap().filetype {
            if t.type_() == mime::IMAGE {
                return Some(AppAction::DownloadFile);
            }
        }

        None
    }

    fn add_user_message(&mut self, user_log: VerifyingKey, msg: UserMessage) {
        let mut db = self.db.lock().unwrap();
        let log = db.messages.entry(user_log).or_insert(Vec::new());
        log.push(msg);
    }

    async fn parse_cmd(&mut self, cmd: &str) -> Result<Option<AppAction>> {
        let args = shlex::split(cmd)
            .ok_or(eyre::Report::msg("error: Invalid quoting"))?;
        let cli = Cli::try_parse_from(args).map_err(eyre::Report::msg)?;

        let action = match cli.command {
            Command::Share { path } => AppAction::ShareFile(path),
            Command::Accept => AppAction::DownloadFile,
        };

        Ok(Some(action))
    }

    async fn share_file(&mut self, path: PathBuf) -> Result<()> {
        let handle = FileHandle::new(path).await?;
        {
            let mut db = self.db.lock().unwrap();
            db.add_file(handle.clone());
        }
        let to = self.tui.get_current_user();
        let msg = PeerMessageData::FileMeta(handle.get_metadata().clone());

        self.parse_file(handle.get_metadata().clone(), handle.get_path().to_owned()).await?;
        self.send_message(msg, to).await
    }

    async fn get_file(&mut self) -> Result<()> {
        let to = self.tui.get_current_user();
        self.conn_manager.send(to, PeerCommand::GetFile).await;
        Ok(())
    }

    async fn send_message(
        &mut self,
        msg: PeerMessageData,
        to: VerifyingKey,
    ) -> Result<()> {
        let identity = {
            let db = self.db.lock().unwrap();
            db.myself.clone()
        };

        let user_msg = UserMessage::new(identity.get_public_key(), msg.clone());

        self.tui.add_user_message(to, &user_msg);
        self.add_user_message(to, user_msg);
        self.conn_manager.send(to, PeerCommand::Send(msg)).await;

        Ok(())
    }

    async fn parse_file(&mut self, meta: FileMetadata, path: PathBuf) -> Result<()> {
        if let Some(mime) = &meta.filetype {
            if mime.type_() == mime::IMAGE {
                let image = tokio::task::spawn_blocking(
                    move || -> Result<DynamicImage, ImageError> {
                        event!(Level::DEBUG, "Trying to open: {}", path.display());
                        ImageReader::open(&path)?.decode()
                    },
                )
                .await?
                .unwrap();

                event!(Level::DEBUG, "Decoded an image!");

                self.tui.add_image(meta.hash, image);
            }
        }

        Ok(())
    }

    async fn execute(
        &mut self,
        action: AppAction,
    ) -> Result<Option<AppAction>> {
        let result = match action {
            AppAction::Quit => {
                self.token.cancel();
                self.tracker.close();
                None
            }
            AppAction::Redraw => {
                self.tui.draw(self.terminal)?;
                None
            }
            AppAction::TuiAction(action) => self.tui.react(action)?,
            AppAction::SelectUser(user) => {
                self.tui.select_user(user);
                None
            }
            AppAction::ReceiveMessage(msg) => self.receive_message(msg),
            AppAction::ParseCommand(cmd) => {
                if cmd.chars().nth(0).unwrap() != '/' {
                    Some(AppAction::SendTextMessage(cmd))
                } else {
                    self.parse_cmd(&cmd[1..]).await?
                }
            }
            AppAction::SendPeerMessage(msg_data, peer) => {
                self.send_message(msg_data, peer).await?;
                None
            }
            AppAction::SendTextMessage(msg_str) => {
                let msg_data = PeerMessageData::Text(msg_str);
                let peer = self.tui.get_current_user();
                Some(AppAction::SendPeerMessage(msg_data, peer))
            }
            AppAction::ShareFile(path) => {
                self.share_file(path).await?;
                None
            }
            AppAction::DownloadFile => {
                self.get_file().await?;
                None
            }
            AppAction::ReceiveDownloadedFile => {
                let meta = self.pending_download.as_ref().unwrap();
                self.parse_file(meta.clone(), meta.get_save_path()).await?;

                None
            }
            AppAction::SetConnected => {
                self.tui.set_connected();
                None
            }
            AppAction::SetConnecting => {
                self.tui.set_connecting();
                None
            }
            AppAction::SetOffline => {
                self.tui.set_offline();
                None
            }
        };

        Ok(result)
    }
}
