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
type MaybeLine = Option<(usize, Span)>;


#[derive(Debug)]
struct Span {
    lo: usize,
    hi: usize
}

impl Span {
    fn from(original: &String, slice: &str) -> Span {
        let base = original.as_str() as *const str as *const () as usize;
        let lo = slice as *const str as *const () as usize - base;
        let hi = lo + slice.len();

        Span { lo, hi }
    }
}


#[derive(Debug)]
pub struct AppUI<'a> {
    area: Rect,
    scroll_pos: u16,
    max_scroll: u16,
    textarea: TextArea<'a>,
    messages: Vec<String>,
    lines: VecDeque<MaybeLine>,
    must_regenerate: bool
}

impl<'a> AppUI<'a> {
    pub fn new(messages: Vec<String>, terminal: &mut Term) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_block(Block::bordered());
        textarea.set_cursor_line_style(Style::default());
        let area = terminal.get_frame().area();
        
        AppUI {
            area,
            scroll_pos: 0,
            max_scroll: 0,
            textarea,
            must_regenerate: true,
            lines: VecDeque::new(),
            messages
        }
    }

    pub fn draw(&mut self, terminal: &mut Term) -> io::Result<()> {
        terminal.draw(|frame| {
            let new_area = frame.area();
            if new_area != self.area {
                self.must_regenerate = true;
            }
            self.area = new_area;

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![
                    Constraint::Min(0),
                    Constraint::Length(3),
                ])
                .split(self.area);

            if self.must_regenerate {
                self.resize(&layout);
                self.must_regenerate = false;
            }

            let items: Vec<Line> = self.lines.iter()
                .map(|line| Line::from(self.get_slice(line)))
                .collect();

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

    fn resize(&mut self, layout: &Rc<[Rect]>) {
        self.lines = self.messages.iter().zip(0..self.messages.len())
            .map(|(msg, idx)| {
                textwrap::wrap(msg, layout[0].width as usize)
                    .into_iter()
                    .map(move |s| Some((idx, Span::from(msg, s.as_ref()))))
            })
            .flatten()
            .collect();

        while (self.lines.len() as u16) < layout[0].height {
            self.lines.push_front(None);
        }
        
        self.max_scroll = self.lines.len() as u16 - layout[0].height;
        if self.scroll_pos > self.max_scroll {
            self.scroll_pos = self.max_scroll;
        }
    }

    fn get_slice(&self, line: &MaybeLine) -> &str {
        line.as_ref().map_or(
            "",
            |(idx, span)| &self.messages[*idx][span.lo..span.hi]
        )
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
