use crate::message::DisplayMessage;
use crate::messageview::{MessageView, MessageViewAction};
use crate::friendsview::{DisplayUser, FriendsView, FriendsViewAction};
use crate::component::Component;
use crate::eventmanager::PressedKey;
use std::sync::{Arc, Mutex};
use ed25519_dalek::VerifyingKey;
use libchatty::identity::UserDb;
use libchatty::messaging::{PeerMessageData, UserMessage};

use std::io::{self, Stdout};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    backend::CrosstermBackend,
    prelude::*,
    widgets::Tabs,
    Terminal,
};

type Term = Terminal<CrosstermBackend<Stdout>>;


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
        let friends: Vec<DisplayUser> = {
            let db = db.lock().unwrap();
            db.remote.iter()
                .map(|(k, v)| DisplayUser {
                    name: v.name.clone(),
                    surname: v.surname.clone(),
                    key: k.clone()
                })
                .collect()
        };
        
        Self {
            message_view: MessageView::new(Vec::new()),
            friends_view: FriendsView::new(friends),
            selected_tab: SelectedTab::Friends,
            db
        }
    }

    pub fn get_current_user(&self) -> VerifyingKey {
        self.friends_view.get_selected_user().unwrap()
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

    pub fn add_message(&mut self, to: VerifyingKey, msg: &UserMessage) {
        if let Some(user) = self.friends_view.get_selected_user() {
            if user == to {
                let user_meta = {
                    let db = self.db.lock().unwrap();
                    db.remote.get(&to).unwrap().clone()
                };

                let text = match &msg.content {
                    PeerMessageData::Text(text) => text
                };

                let message = DisplayMessage {
                    content: text.clone(),
                    author: user_meta.nickname,
                    timestamp: msg.timestamp
                };
                
                self.message_view.append(message);
            }
        }
    }
}

pub enum TuiAction {
    Quit,
    SwitchTab,
    MessageViewAction(MessageViewAction),
    FriendsViewAction(FriendsViewAction)
}
