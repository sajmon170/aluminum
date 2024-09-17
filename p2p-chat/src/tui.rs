use crate::messageview::{MessageView, MessageViewAction};
use crate::friendsview::{FriendsView, FriendsViewAction};
use crate::component::Component;
use crate::eventmanager::PressedKey;
use std::sync::{Arc, Mutex};
use libchatty::identity::UserDb;

use std::{
    collections::VecDeque,
    io::{self, Stdout},
    marker::PhantomData,
    rc::Rc,
};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    backend::CrosstermBackend,
    crossterm::event::KeyEvent,
    prelude::*,
    widgets::{Block, Paragraph, Tabs},
    Terminal,
};

type Term = Terminal<CrosstermBackend<Stdout>>;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumIter, Display, FromRepr, EnumCount as EnumCountMacro};

pub struct Tui<'a> {
    message_view: MessageView<'a>,
    selected_tab: SelectedTab,
    friends_view: FriendsView,
    db: Arc<Mutex<UserDb>>
}

// TODO - port this to an actor architecture
// Main problem - how do we pass the terminal here?

use crate::controller::AppAction;

#[derive(Copy, Clone, Display, EnumIter, FromRepr, EnumCountMacro)]
enum SelectedTab {
    #[strum(to_string = "Friends")]
    Friends,
    #[strum(to_string = "Messages")]
    Messages
}

impl SelectedTab {
    fn next(self) -> Self {
        let current_idx = self as usize;
        let next_idx = (current_idx + 1) % Self::COUNT;
        Self::from_repr(next_idx).unwrap()
    }
}

impl<'a> Tui<'a> {
    pub fn new(db: Arc<Mutex<UserDb>>) -> Self {
        Self {
            message_view: MessageView::new(Vec::new()),
            friends_view: FriendsView::new(Vec::new()),
            selected_tab: SelectedTab::Friends,
            db
        }
    }

    pub fn draw(&mut self, terminal: &mut Term) -> io::Result<()> {
        terminal.draw(|frame| {
            let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(2),
                Constraint::Min(0),
            ])
                .split(frame.area());

            let titles = SelectedTab::iter().map(|tab| tab.to_string());
            let tabs = Tabs::new(titles)
                .select(self.selected_tab as usize)
                .padding("", "")
                .divider(" ");

            frame.render_widget(tabs, layout[0]);

            // Big problem - can't use enum dispatch because of the associated type
            // This needs to be fixed immediately!!!
            match self.selected_tab {
                SelectedTab::Friends => self.friends_view.draw(frame, layout[1]),
                SelectedTab::Messages => self.message_view.draw(frame, layout[1])
            }
        })?;

        Ok(())
    }

    pub fn handle_kbd_event(&mut self, key: PressedKey) -> Option<AppAction> {
        // TODO - pick an event handler based on active tab
        //      - Streamline the AppAction type - either combine all into one type
        //        or match based on currently selected
        if key.code == KeyCode::Char('q')
            && key.modifiers == KeyModifiers::CONTROL
        {
            Some(AppAction::Quit)
        }
        else if key.code == KeyCode::Tab {
            Some(AppAction::TuiAction(TuiAction::SwitchTab))
        }
        else {
            match self.selected_tab {
                SelectedTab::Friends => {
                    self.friends_view.handle_kbd_event(key)
                        .and_then(|action|
                            Some(AppAction::TuiAction(TuiAction::FriendsViewAction(action))))
                }
                SelectedTab::Messages => {
                    self.message_view.handle_kbd_event(key)
                        .and_then(|action|
                            Some(AppAction::TuiAction(TuiAction::MessageViewAction(action))))
                }
            }
        }
    }

    pub fn react(&mut self, action: TuiAction) -> io::Result<()> {
        match action {
            TuiAction::Quit => { },
            TuiAction::SwitchTab => { self.next_tab() }
            // TODO - fix this architecture - the action should be handled in a
            // single place, NOT in two!
            TuiAction::MessageViewAction(action) => self.message_view.react(action)?,
            TuiAction::FriendsViewAction(action) => {
                match action {
                    FriendsViewAction::SelectCurrentUser => {
                        self.friends_view.react(action)?;
                        if let Some(key) = self.friends_view.get_selected_user() {
                            // load messages to message view
                        }
                        
                    }
                    _ => {
                        self.friends_view.react(action)?;
                    }
                }
            },
        }

        Ok(())
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = self.selected_tab.next();
    }

    /*
    pub async fn run(&mut self) -> io::Result<()> {
        loop {
            tokio::select! {
                Some(message) = self.rx.recv() => {
                    match message {
                        TuiMessage::Redraw => self.draw()?,
                        _ => { }
                    }
                },
                _ = self.token.cancelled() => { break; },
                else => { self.token.cancel() }
            }
        }

        Ok(())
        }
        */
}

pub enum TuiAction {
    Quit,
    SwitchTab,
    MessageViewAction(MessageViewAction),
    FriendsViewAction(FriendsViewAction)
}

/*
enum TuiMessage {
    Redraw,
    KbdEvent(PressedKey, oneshot::Sender<TuiAction>),
    Action(TuiAction),
}

#[derive(Debug)]
pub struct TuiHandle {
    pub event_tx: mpsc::Sender<TuiMessage>,
}

impl TuiHandle {
    pub fn new(tracker: &TaskTracker, token: CancellationToken) -> Self {
        let (event_tx, event_rx) = mpsc::channel(32);
        let mut tui = Tui::new();

        tracker.spawn(async move { event_mgr.handle_events().await });

        Self { event_tx }
    }
}
*/
