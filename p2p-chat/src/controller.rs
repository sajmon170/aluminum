use std::{
    io::{self, Stdout},
    sync::{Arc, Mutex},
};

use ed25519_dalek::VerifyingKey;
use tokio::sync::mpsc;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::mem;

use crate::{
    connmanager::{ConnInstruction, ConnManagerHandle, ConnMessage},
    eventmanager::{AppEvent, EventManagerHandle},
    messageview::MessageViewAction,
    tui::{Tui, TuiAction},
    action::AppAction
};

use libchatty::{
    identity::{Myself, Relay, UserDb},
    messaging::{PeerMessageData, UserMessage},
};

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

    pub async fn run(&mut self) -> io::Result<()> {
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
            },
            AppEvent::SetConnected => Some(AppAction::SetConnected),
            AppEvent::SetConnecting => Some(AppAction::SetConnecting),
            AppEvent::SetOffline => Some(AppAction::SetOffline)
        }
    }

    fn receive_message(&mut self, msg: UserMessage) {
        self.tui.add_message(msg.author, &msg);
        self.add_message(msg.author, msg);
    }

    fn add_message(&mut self, user_log: VerifyingKey, msg: UserMessage) {
        let mut db = self.db.lock().unwrap();
        let log = db.messages.entry(user_log).or_insert(Vec::new());
        log.push(msg);
    }

    fn parse_cmd(&self, cmd: &str) -> io::Result<Option<AppAction>> {
        Ok(None)
    }

    async fn send_message(
        &mut self,
        msg: PeerMessageData,
        to: VerifyingKey,
    ) -> io::Result<()> {
        let identity = {
            let db = self.db.lock().unwrap();
            db.myself.clone()
        };

        let user_msg = UserMessage::new(identity.get_public_key(), msg.clone());

        self.tui.add_message(to, &user_msg);
        self.add_message(to, user_msg);

        self.conn_manager
            .tx
            .send(ConnInstruction::Send(to, msg))
            .await
            .unwrap();

        Ok(())
    }

    async fn execute(&mut self, action: AppAction) -> io::Result<Option<AppAction>> {
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
            AppAction::ParseCommand(cmd) => {
                if cmd.chars().nth(0).unwrap() != '/' {
                    let msg_data = PeerMessageData::Text(cmd);
                    Some(AppAction::SendTextMessage(msg_data, self.tui.get_current_user()))
                }
                else {
                    self.parse_cmd(&cmd[1..])?
                }
            }
            AppAction::SendTextMessage(msg_data, pubkey) => {
                self.send_message(msg_data, pubkey).await?;
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
