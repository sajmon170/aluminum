use crate::{
    component::Component,
    action,
    eventmanager::PressedKey
};

use ed25519_dalek::VerifyingKey;
use ratatui::{
    crossterm::event::KeyCode,
    prelude::*,
    widgets::{Row, Table, TableState},
};

use base64::prelude::*;

use color_eyre::Result;

pub struct FriendsView {
    state: TableState,
    users: Vec<DisplayUser>,
    selected_user: Option<VerifyingKey>,
}

pub struct DisplayUser {
    pub name: String,
    pub surname: String,
    pub key: VerifyingKey,
}

// TODO - optimize the string allocations away
impl DisplayUser {
    pub fn get_full_display_name(&self) -> String {
        format!("{} {}", self.name, self.surname)
    }

    pub fn get_display_key(&self) -> String {
        BASE64_STANDARD.encode(self.key.as_bytes())
    }
}

impl FriendsView {
    pub fn new(users: Vec<DisplayUser>) -> Self {
        let selected_user = users.first().and_then(|user| Some(user.key));

        Self {
            state: TableState::new(),
            users,
            selected_user,
        }
    }

    pub fn select_current_user(&mut self) {
        self.selected_user = self
            .state
            .selected()
            .and_then(|idx| self.users.get(idx))
            .and_then(|user| Some(user.key))
    }

    pub fn get_selected_user(&self) -> Option<VerifyingKey> {
        self.selected_user
    }
}

impl Widget for &mut FriendsView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let widths = [Constraint::Length(25), Constraint::Min(0)];

        let rows = self.users.iter().map(|user| {
            Row::new(vec![user.get_full_display_name(), user.get_display_key()])
        });

        let table = Table::new(rows, widths)
            .highlight_style(Style::new().fg(Color::Black).bg(Color::White));

        StatefulWidget::render(table, area, buf, &mut self.state);
    }
}

pub enum FriendsViewAction {
    SelectNext,
    SelectPrev,
    SelectCurrentUser,
}

impl Component for FriendsView {
    type Action = FriendsViewAction;
    type AppAction = action::AppAction;

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(self, area);
    }

    fn handle_kbd_event(&mut self, key: PressedKey) -> Option<Self::Action> {
        if key.code == KeyCode::Down {
            Some(Self::Action::SelectNext)
        }
        else if key.code == KeyCode::Up {
            Some(Self::Action::SelectPrev)
        }
        else if key.code == KeyCode::Enter && !self.users.is_empty() {
            Some(Self::Action::SelectCurrentUser)
        }
        else {
            None
        }
    }

    fn react(&mut self, action: Self::Action) -> Result<Option<Self::AppAction>> {
        let result = match action {
            Self::Action::SelectNext => {
                self.state.select_next();
                None
            },
            Self::Action::SelectPrev => {
                self.state.select_previous();
                None
            },
            Self::Action::SelectCurrentUser => {
                self.select_current_user();
                let selected = self.get_selected_user().unwrap();
                Some(Self::AppAction::SelectUser(selected))
            }
        };

        Ok(result)
    }
}
