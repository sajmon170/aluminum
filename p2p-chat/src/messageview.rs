use std::{
    collections::VecDeque,
    io::{self},
    rc::Rc,
};

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::*,
    widgets::{Block, Paragraph},
};

use tui_textarea::TextArea;

use crate::component::Component;
use crate::eventmanager::PressedKey;
use crate::message::DisplayMessage;

#[derive(Debug)]
pub struct MessageView<'a> {
    scroll_pos: u16,
    max_scroll: u16,
    textarea: TextArea<'a>,
    messages: Vec<String>,
}

// TODO
// - either replace textwrap with .wrap()
// OR
// - use textwrap as normal but print the formatted name first,
//   then print the wrapped text with the name characters skipped.
//
// Add a color field to the DisplayMessage struct
// OR - add a style type field.
// available styles: Recipient, Responder

impl<'a> Widget for &mut MessageView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        let items = MessageView::get_lines(&layout, &self.messages);

        self.max_scroll = items.len() as u16 - layout[0].height;
        if self.scroll_pos > self.max_scroll {
            self.scroll_pos = self.max_scroll;
        }

        Paragraph::new(Text::from(items))
            .scroll((self.max_scroll - self.scroll_pos, 0))
            .render(layout[0], buf);

        Widget::render(&self.textarea, layout[1], buf);
    }
}

// TODO:
// - load messages in a better way
impl<'a> MessageView<'a> {
    pub fn new(messages: Vec<String>) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_block(Block::bordered());
        textarea.set_cursor_line_style(Style::default());

        Self {
            scroll_pos: 0,
            max_scroll: 0,
            textarea,
            messages,
        }
    }

    // TODO - Convert this to iterators (i.e. - remove the VecDeque allocation)
    fn get_lines<'b>(
        layout: &Rc<[Rect]>,
        messages: &'b Vec<String>,
    ) -> Vec<Line<'b>> {
        let mut lines: VecDeque<Line> = messages
            .iter()
            .zip(0..messages.len())
            .map(|(msg, idx)| {
                textwrap::wrap(msg, layout[0].width as usize)
                    .into_iter()
                    .map(|s| Line::raw(s))
            })
            .flatten()
            .collect();

        while (lines.len() as u16) < layout[0].height {
            lines.push_front(Line::from(""));
        }

        lines.into()
    }

    pub fn append(&mut self, msg: DisplayMessage) {
        self.messages.push(msg.to_string());
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_pos > 0 {
            self.scroll_pos = self.scroll_pos - 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_pos < self.max_scroll as u16 {
            self.scroll_pos = self.scroll_pos + 1;
        }
    }

    pub fn write_key(&mut self, key: PressedKey) {
        self.textarea.input(KeyEvent::from(key));
    }

    pub fn extract_msg(&mut self) -> Option<String> {
        if self.textarea.is_empty() {
            None
        }
        else {
            self.textarea.delete_line_by_head();
            Some(self.textarea.yank_text())
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

#[derive(Debug)]
pub enum MessageViewAction {
    ScrollUp,
    ScrollDown,
    //ReceiveMsg(String),
    SendMsg(String),
    WriteKey(PressedKey),
}

impl<'a> Component for MessageView<'a> {
    type Action = MessageViewAction;

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(self, area);
    }

    fn handle_kbd_event(&mut self, key: PressedKey) -> Option<Self::Action> {
        if key.code == KeyCode::Down {
            Some(Self::Action::ScrollDown)
        }
        else if key.code == KeyCode::Up {
            Some(Self::Action::ScrollUp)
        }
        else if key.code == KeyCode::Enter {
            self.extract_msg()
                .map_or(None, |msg| Some(Self::Action::SendMsg(msg)))
        }
        else {
            Some(Self::Action::WriteKey(key))
        }
    }

    fn react(&mut self, action: Self::Action) -> io::Result<()> {
        match action {
            Self::Action::ScrollUp => self.scroll_up(),
            Self::Action::ScrollDown => self.scroll_down(),
            Self::Action::WriteKey(key) => self.write_key(key),
            _ => {}
        }
        Ok(())
    }
}
