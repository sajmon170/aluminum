use std::{
    io::{self, Stdout},
    sync::{Arc, Mutex},
};

use ed25519_dalek::VerifyingKey;
use tokio::sync::mpsc;

use tokio_util::{sync::CancellationToken, task::TaskTracker};

use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    connmanager::{ConnInstruction, ConnManagerHandle, ConnMessage},
    eventmanager::{AppEvent, EventManagerHandle},
    messageview::MessageViewAction,
    tui::{Tui, TuiAction},
};

use libchatty::{
    identity::{Myself, Relay, UserDb},
    messaging::{PeerMessageData, UserMessage},
};

use tracing::{event, Level};

type Term = Terminal<CrosstermBackend<Stdout>>;

pub enum AppAction {
    Quit,
    Redraw,
    TuiAction(TuiAction),
    ReceiveMessage(UserMessage),
    SendMessage(PeerMessageData, VerifyingKey),
    SetOffline,
    SetConnecting,
    SetConnected,
}

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
                    if let Some(action) = self.handle_event(event) {
                        self.execute(action).await?;
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

    async fn execute(&mut self, action: AppAction) -> io::Result<()> {
        match action {
            AppAction::Quit => {
                self.token.cancel();
                self.tracker.close();
            }
            AppAction::Redraw => {
                self.tui.draw(self.terminal)?;
            }
            AppAction::TuiAction(action) => {
                match action {
                    TuiAction::MessageViewAction(
                        MessageViewAction::SendMsg(msg),
                    ) => {
                        let data = PeerMessageData::Text(msg);
                        self.send_message(data, self.tui.get_current_user())
                            .await?;
                        //self.execute(AppAction::SendMessage(data, self.tui.get_current_user())).await?;
                    }
                    _ => self.tui.react(action)?,
                };
            }
            AppAction::ReceiveMessage(msg) => {
                self.receive_message(msg);
            }
            AppAction::SendMessage(msg_data, pubkey) => {
                let _ = self.send_message(msg_data, pubkey).await;
            }
            AppAction::SetConnected => {
                self.tui.set_connected();
            }
            AppAction::SetConnecting => {
                self.tui.set_connecting();
            }
            AppAction::SetOffline => {
                self.tui.set_offline();
            }
        };

        Ok(())
    }
}
