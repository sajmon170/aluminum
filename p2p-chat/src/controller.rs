use std::{
    ffi::OsString,
    io::{self, Stdout},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tokio::sync::mpsc;

use tokio_util::{sync::CancellationToken, task::TaskTracker};

use ratatui::{
    backend::CrosstermBackend,
    crossterm::event::{KeyCode, KeyModifiers},
    Terminal,
};

use crate::{
    connmanager::{ConnInstruction, ConnManagerHandle},
    eventmanager::{AppEvent, EventManagerHandle, PressedKey},
    ui::AppUI,
};

use libchatty::identity::{Myself, User, UserDb, Relay};

use tracing::{event, Level};

type Term = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug)]
enum Action {
    Quit,
    Redraw,
    ScrollUp,
    ScrollDown,
    ReceiveMsg(String),
    SendMsg(String),
    WriteKey(PressedKey),
}

#[derive(Debug)]
pub struct AppController<'a> {
    terminal: &'a mut Term,
    ui: AppUI<'a>,
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
        relay: Relay
    ) -> Self {
        let ui = AppUI::new(msgs, terminal);
        let (message_tx, message_rx) = mpsc::channel(32);
        let event_manager =
            EventManagerHandle::new(message_rx, &tracker, token.clone());

        let identity: Myself;
        {
            let db = db.lock().unwrap();
            identity = db.myself.clone();
        }
        
        let conn_manager = ConnManagerHandle::new(
            identity,
            relay,
            message_tx,
            &tracker,
            token.clone(),
        );

        Self {
            terminal,
            ui,
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

        Ok(())
    }

    fn handle_event(&mut self, event: AppEvent) -> Option<Action> {
        match event {
            AppEvent::FrameTick => Some(Action::Redraw),
            AppEvent::KeyPress(key) => self.handle_kbd_event(key),
            AppEvent::ReceiveMessage(msg) => Some(Action::ReceiveMsg(msg)),
            _ => None,
        }
    }

    fn handle_kbd_event(&mut self, key: PressedKey) -> Option<Action> {
        if key.code == KeyCode::Char('q')
            && key.modifiers == KeyModifiers::CONTROL
        {
            Some(Action::Quit)
        } else if key.code == KeyCode::Down {
            Some(Action::ScrollDown)
        } else if key.code == KeyCode::Up {
            Some(Action::ScrollUp)
        } else if key.code == KeyCode::Enter {
            self.ui
                .extract_msg()
                .map_or(None, |msg| Some(Action::SendMsg(msg)))
        } else {
            Some(Action::WriteKey(key))
        }
    }

    async fn execute(&mut self, action: Action) -> io::Result<()> {
        match action {
            Action::Quit => {
                self.token.cancel();
                self.tracker.close();
            }
            Action::Redraw => self.ui.draw(&mut self.terminal)?,
            Action::ScrollUp => self.ui.scroll_up(),
            Action::ScrollDown => self.ui.scroll_down(),
            Action::WriteKey(key) => self.ui.write_key(key),
            Action::ReceiveMsg(msg) => self.ui.write_msg(msg.clone()),
            Action::SendMsg(msg) => {
                self.ui.write_msg(msg.clone());
                /*
                self.conn_manager
                    .tx
                    .send(ConnInstruction::Send(msg))
                    .await
                    .unwrap();
*/
                let id = {
                    let db = self.db.lock().unwrap();
                    db.myself.get_public_key()
                };

                self.conn_manager
                    .tx
                    .send(ConnInstruction::GetUser(id))
                    .await
                    .unwrap();
            }
        };

        Ok(())
    }
}
