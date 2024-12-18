use libchatty::messaging::UserMessage;

use tokio::{
    sync::mpsc,
    time::{self, Duration},
};

use futures::stream::StreamExt;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use ratatui::crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers};

use crate::connmanager::ConnMessage;

#[derive(Debug)]
pub struct PressedKey {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl From<PressedKey> for KeyEvent {
    fn from(key: PressedKey) -> KeyEvent {
        KeyEvent::new(key.code, key.modifiers)
    }
}

#[derive(Debug)]
pub enum AppEvent {
    ReceiveMessage(UserMessage),
    NotifyDownloaded,
    SetOffline,
    SetConnecting,
    SetConnected,
    FrameTick,
    KeyPress(PressedKey),
}

#[derive(Debug)]
struct EventManager {
    event_tx: mpsc::Sender<AppEvent>,
    msg_rx: mpsc::Receiver<ConnMessage>,
    token: CancellationToken,
}

impl EventManager {
    async fn handle_events(&mut self) {
        let mut framerate = time::interval(Duration::from_millis(16));
        let mut event_stream = crossterm::event::EventStream::new();

        loop {
            tokio::select! {
                _ = framerate.tick() => {
                    let _ = self.event_tx.send(AppEvent::FrameTick).await;
                },
                Some(msg) = self.msg_rx.recv() => {
                    let event = match msg {
                        ConnMessage::UserMessage(msg) => AppEvent::ReceiveMessage(msg),
                        ConnMessage::DownloadedFile => AppEvent::NotifyDownloaded,
                        ConnMessage::ServerOffline => AppEvent::SetOffline,
                        ConnMessage::Connecting => AppEvent::SetConnecting,
                        ConnMessage::Connected => AppEvent::SetConnected
                    };

                    self.event_tx.send(event).await.unwrap();
                }
                Some(event) = event_stream.next() => {
                    if let Ok(event::Event::Key(key)) = event {
                        self.event_tx.send(AppEvent::KeyPress(PressedKey {
                            code: key.code,
                            modifiers: key.modifiers
                        })).await.unwrap();
                    }
                }
                _ = self.token.cancelled() => { break; }
            };
        }
    }
}

#[derive(Debug)]
pub struct EventManagerHandle {
    pub event_rx: mpsc::Receiver<AppEvent>,
}

impl EventManagerHandle {
    pub fn new(
        msg_rx: mpsc::Receiver<ConnMessage>,
        tracker: &TaskTracker,
        token: CancellationToken,
    ) -> EventManagerHandle {
        let (event_tx, event_rx) = mpsc::channel(32);
        let mut event_mgr = EventManager {
            event_tx,
            msg_rx,
            token,
        };

        tracker.spawn(async move {
            event_mgr.handle_events().await;
        });

        EventManagerHandle { event_rx }
    }
}
