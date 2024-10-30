use std::{
    io::{self, Stdout}, path::PathBuf, sync::{Arc, Mutex}
};

use ed25519_dalek::VerifyingKey;
use tokio::{sync::mpsc, fs::File};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    connmanager::ConnManagerHandle,
    peermanager::PeerCommand,
    eventmanager::{AppEvent, EventManagerHandle},
    messageview::MessageViewAction,
    messagerepl::{Cli, Command, Parser},
    tui::{Tui, TuiAction},
    action::AppAction
};

use libchatty::{
    identity::{Myself, Relay, UserDb},
    messaging::{PeerMessageData, UserMessage},
};

use tracing::{event, Level};

use color_eyre::Result;

type Term = Terminal<CrosstermBackend<Stdout>>;

pub struct AppController<'a> {
    terminal: &'a mut Term,
    tui: Tui<'a>,
    event_manager: EventManagerHandle,
    conn_manager: ConnManagerHandle,
    tracker: TaskTracker,
    token: CancellationToken,
    db: Arc<Mutex<UserDb>>,
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
        let tui = Tui::new(db.clone());
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
        );

        Self {
            terminal,
            tui,
            event_manager,
            conn_manager,
            tracker,
            token,
            db,
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
            AppEvent::ReceiveMessage(msg) => Some(AppAction::ReceiveMessage(msg)),
            AppEvent::ReceiveInvite(invite) => Some(AppAction::ShowInvite(invite)),
            AppEvent::NotifyDownloaded => Some(AppAction::ShowDownloadNotification),
            AppEvent::SetConnected => Some(AppAction::SetConnected),
            AppEvent::SetConnecting => Some(AppAction::SetConnecting),
            AppEvent::SetOffline => Some(AppAction::SetOffline)
        }
    }

    fn receive_message(&mut self, msg: UserMessage) {
        self.tui.add_user_message(msg.author, &msg);
        self.add_user_message(msg.author, msg);
    }

    fn add_user_message(&mut self, user_log: VerifyingKey, msg: UserMessage) {
        let mut db = self.db.lock().unwrap();
        let log = db.messages.entry(user_log).or_insert(Vec::new());
        log.push(msg);
    }

    async fn parse_cmd(&mut self, cmd: &str) -> Result<Option<AppAction>> {
        let args = shlex::split(cmd).ok_or(eyre::Report::msg("error: Invalid quoting"))?;
        let cli = Cli::try_parse_from(args).map_err(eyre::Report::msg)?;

        // This should return an AppAction
        match cli.command {
            Command::Img { path } => self.send_image(path).await?,
            Command::Share { path } => self.share_file(path).await?,
            Command::Accept => self.get_file().await?
        };
        
        Ok(None)
    }

    async fn send_image(&mut self, path: PathBuf) -> Result<()> {
        Ok(())
    }

    async fn share_file(&mut self, path: PathBuf) -> Result<()> {
        let to = self.tui.get_current_user();
        self.conn_manager.send(to, PeerCommand::ShareFile(path)).await;
        Ok(())
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

    async fn execute(&mut self, action: AppAction) -> Result<Option<AppAction>> {
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
            AppAction::TuiAction(action) => {
                self.tui.react(action)?
            }
            AppAction::SelectUser(user) => {
                self.tui.select_user(user);
                None
            }
            AppAction::ReceiveMessage(msg) => {
                self.receive_message(msg);
                None
            }
            AppAction::ShowInvite(invite) => {
                self.tui.show_invite(invite);
                None
            }
            AppAction::ShowDownloadNotification => {
                self.tui.show_download_notification();
                None
            }
            AppAction::ParseCommand(cmd) => {
                if cmd.chars().nth(0).unwrap() != '/' {
                    Some(AppAction::SendTextMessage(cmd))
                }
                else {
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
            AppAction::SendImageMessage(path) => {
                self.send_image(path).await?;
                None
            }
            AppAction::ShareFile(path) => {
                self.share_file(path).await?;
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
