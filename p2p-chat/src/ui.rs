use std::{
    io::{self, Stdout},
    rc::Rc,
    collections::VecDeque,
};

use chrono::Utc;

use ratatui::{
    prelude::*,
    backend::CrosstermBackend,
    crossterm::event::KeyEvent,
    widgets::{Block, Paragraph},
    Terminal
};

use tui_textarea::TextArea;

use crate::message::DisplayMessage;
use crate::eventmanager::PressedKey;

type Term = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug)]
pub struct AppUI<'a> {
    scroll_pos: u16,
    max_scroll: u16,
    textarea: TextArea<'a>,
    messages: Vec<String>,
    must_regenerate: bool
}

impl<'a> AppUI<'a> {
    pub fn new(messages: Vec<String>, terminal: &mut Term) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_block(Block::bordered());
        textarea.set_cursor_line_style(Style::default());
        
        AppUI {
            scroll_pos: 0,
            max_scroll: 0,
            textarea,
            must_regenerate: true,
            messages
        }
    }

    pub fn draw(&mut self, terminal: &mut Term) -> io::Result<()> {
        terminal.draw(|frame| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![
                    Constraint::Min(0),
                    Constraint::Length(3),
                ])
                .split(frame.area());

            let items = Self::get_lines(&layout, &self.messages);

            self.max_scroll = items.len() as u16 - layout[0].height;
            if self.scroll_pos > self.max_scroll {
                self.scroll_pos = self.max_scroll;
            }

            frame.render_widget(
                Paragraph::new(Text::from(items))
                    .scroll((self.max_scroll - self.scroll_pos, 0)),
                layout[0]
            );
            frame.render_widget(
                &self.textarea,
                layout[1]
            );
        })?;

        Ok(())
    }

    fn get_lines<'b>(layout: &Rc<[Rect]>, messages: &'b Vec<String>) -> Vec<Line<'b>> {
        let mut lines: VecDeque<Line> = messages.iter().zip(0..messages.len())
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

    // TODO: Implement proper message formatting!
    // Not every message is from me!
    pub fn write_msg(&mut self, msg: String) {
        self.must_regenerate = true;
        self.messages.push(DisplayMessage {
            content: msg,
            author: String::from("Me"),
            timestamp: Utc::now()
        }.to_string());
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
}
