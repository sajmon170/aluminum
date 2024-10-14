use crate::component::Component;
use crate::eventmanager::PressedKey;
use crate::friendsview::{DisplayUser, FriendsView, FriendsViewAction};
use crate::message::DisplayMessage;
use crate::messageview::{MessageView, MessageViewAction};
use ed25519_dalek::VerifyingKey;
use libchatty::identity::UserDb;
use libchatty::messaging::{PeerMessageData, UserMessage};
use ratatui::widgets::{Block, Borders, Padding, Paragraph};
use std::sync::{Arc, Mutex};

use std::io::{self, Stdout};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, prelude::*, widgets::Tabs, Terminal};

type Term = Terminal<CrosstermBackend<Stdout>>;

use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount as EnumCountMacro, EnumIter, FromRepr};

pub struct Tui<'a> {
    message_view: MessageView<'a>,
    selected_tab: SelectedTab,
    friends_view: FriendsView,
    db: Arc<Mutex<UserDb>>,
    conn_status: ConnectionStatus
}

// TODO - port this to an actor architecture
// Main problem - how do we pass the terminal here?

use crate::controller::AppAction;

#[derive(Copy, Clone, Display, EnumIter, FromRepr, EnumCountMacro)]
enum SelectedTab {
    #[strum(to_string = "Friends")]
    Friends,
    #[strum(to_string = "Messages")]
    Messages,
}

#[derive(Copy, Clone, Display, EnumIter, FromRepr, EnumCountMacro)]
enum ConnectionStatus {
    #[strum(to_string = "Connecting")]
    Connecting,
    #[strum(to_string = "Connected")]
    Connected,
    #[strum(to_string = "Offline")]
    Offline
}

impl SelectedTab {
    fn next(self) -> Self {
        let current_idx = self as usize;
        let next_idx = (current_idx + 1) % Self::COUNT;
        Self::from_repr(next_idx).unwrap()
    }

    fn title(self) -> String {
        format!("  {self}  ")
    }
}

impl<'a> Tui<'a> {
    pub fn new(db: Arc<Mutex<UserDb>>) -> Self {
        let friends: Vec<DisplayUser> = {
            let db = db.lock().unwrap();
            db.remote
                .iter()
                .map(|(k, v)| DisplayUser {
                    name: v.name.clone(),
                    surname: v.surname.clone(),
                    key: k.clone(),
                })
                .collect()
        };

        Self {
            message_view: MessageView::new(Vec::new()),
            friends_view: FriendsView::new(friends),
            selected_tab: SelectedTab::Friends,
            db,
            conn_status: ConnectionStatus::Connecting
        }
    }

    pub fn get_current_user(&self) -> VerifyingKey {
        self.friends_view.get_selected_user().unwrap()
    }

    fn get_accent_color(&self) -> Color {
        match self.conn_status {
            ConnectionStatus::Connecting => Color::LightYellow,
            ConnectionStatus::Connected => Color::LightGreen,
            ConnectionStatus::Offline => Color::LightRed
        }
    }

    pub fn draw(&mut self, terminal: &mut Term) -> io::Result<()> {
        terminal.draw(|frame| {
            let [top, content] = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(2), Constraint::Min(0)])
                .areas(frame.area());

            self.draw_top_bar(top, frame);

            match self.selected_tab {
                SelectedTab::Friends => {
                    self.friends_view.draw(frame, content)
                }
                SelectedTab::Messages => {
                    self.message_view.draw(frame, content)
                }
            }
        })?;

        Ok(())
    }

    fn draw_top_bar(&self, top: Rect, frame: &mut Frame) {
        let [top, separator] = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Min(0)])
            .areas(top);
        
        let [tab_area, status_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Min(0), Constraint::Length(15)])
            .areas(top);

        let titles = SelectedTab::iter().map(SelectedTab::title);
        let tabs = Tabs::new(titles)
            .highlight_style(Style::default()
                                .bg(self.get_accent_color())
                                .fg(Color::Black))
            .select(self.selected_tab as usize)
            .padding("", "")
            .divider(" ");

        frame.render_widget(tabs, tab_area);

        let border = Block::default()
            .borders(Borders::TOP)
            .border_set(symbols::border::PROPORTIONAL_TALL)
            .style(Style::default().fg(self.get_accent_color()));

        frame.render_widget(border, separator);

        let conn_info = Paragraph::new(Line::from(vec![
            Span::from(self.conn_status.to_string()),
            Span::styled(" ●  ", Style::default().fg(self.get_accent_color()))
        ])).alignment(Alignment::Right);

        frame.render_widget(conn_info, status_area);
        
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
                    self.friends_view.handle_kbd_event(key).and_then(|action| {
                        Some(AppAction::TuiAction(
                            TuiAction::FriendsViewAction(action),
                        ))
                    })
                }
                SelectedTab::Messages => {
                    self.message_view.handle_kbd_event(key).and_then(|action| {
                        Some(AppAction::TuiAction(
                            TuiAction::MessageViewAction(action),
                        ))
                    })
                }
            }
        }
    }

    pub fn react(&mut self, action: TuiAction) -> io::Result<()> {
        match action {
            TuiAction::Quit => {}
            TuiAction::SwitchTab => self.next_tab(),
            // TODO - fix this architecture - the action should be handled in a
            // single place, NOT in two!
            TuiAction::MessageViewAction(action) => {
                self.message_view.react(action)?
            }
            TuiAction::FriendsViewAction(action) => {
                match action {
                    FriendsViewAction::SelectCurrentUser => {
                        self.friends_view.react(action)?;
                        if let Some(key) = self.friends_view.get_selected_user() {
                            self.message_view.clear();
                            self.load_messages(key);
                            self.select_tab(SelectedTab::Messages);
                        }
                    }
                    _ => {
                        self.friends_view.react(action)?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn next_tab(&mut self) {
        self.select_tab(self.selected_tab.next());
    }

    fn select_tab(&mut self, tab: SelectedTab) {
        self.selected_tab = tab;
    }

    pub fn load_messages(&mut self, user: VerifyingKey) {
        let msgs = {
            let db = self.db.lock().unwrap();
            db.messages.get(&user).unwrap_or(&Vec::new()).clone()
        };

        for msg in &msgs {
            self.add_message(self.get_current_user(), msg);
        }
    }

    pub fn add_message(&mut self, to: VerifyingKey, msg: &UserMessage) {
        if let Some(user) = self.friends_view.get_selected_user() {
            if user == to {
                let user_meta = {
                    let db = self.db.lock().unwrap();
                    db.remote
                        .get(&msg.author)
                        .unwrap_or(&db.myself.metadata)
                        .clone()
                };

                let text = match &msg.content {
                    PeerMessageData::Text(text) => text,
                };

                let message = DisplayMessage {
                    content: text.clone(),
                    author: user_meta.nickname,
                    timestamp: msg.timestamp,
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
    FriendsViewAction(FriendsViewAction),
}
