use crate::{
    component::Component,
    action,
    eventmanager::PressedKey,
    message::DisplayMessage
};

use layout::Size;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::*,
    widgets::{Block, Paragraph},
};

use tui_textarea::TextArea;
use tui_scrollview::*;

use color_eyre::Result;

#[derive(Debug)]
pub struct MessageView<'a> {
    textarea: TextArea<'a>,
    messages: Vec<DisplayMessage>,
    scroll_state: ScrollViewState,
    // This flag means that the ScrollView needs to be initialized with data
    // before applying a PageUp/PageDown scroll.
    init_scroll: bool
}

impl<'a> Widget for &mut MessageView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [mut message_log, text_input] = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(0), Constraint::Length(3)])
            .areas(area);

        let width = message_log.width - 1;

        let paragraphs: Vec<(Paragraph, u16)> = self.messages.iter()
            .map(|msg| MessageView::make_paragraph(msg, width))
            .collect();

        let total_height = paragraphs.iter()
            .fold(0, |sum, (_, height)| sum + height);

        let mut scroll_view = ScrollView::new(Size::new(width, total_height));

        let mut starting_height = 0;
        for (paragraph, widget_height) in &paragraphs {
            let area = Rect::new(0, starting_height, width, *widget_height);
            scroll_view.render_widget(paragraph, area);
            starting_height += widget_height;
        }

        if total_height < message_log.height {
            message_log = Rect {
                y: message_log.height - total_height + 2,
                ..message_log
            };
        }

        if self.init_scroll == true {
            StatefulWidget::render(scroll_view.clone(), message_log, buf, &mut self.scroll_state);
            self.reset_scroll();
            self.scroll_state.scroll_up();
            self.init_scroll = false;
        }

        StatefulWidget::render(scroll_view, message_log, buf, &mut self.scroll_state);
        
        Widget::render(&self.textarea, text_input, buf);
    }
}


// TODO:
// - load messages in a better way
impl<'a> MessageView<'a> {
    pub fn new(messages: Vec<DisplayMessage>) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_block(Block::bordered());
        textarea.set_cursor_line_style(Style::default());

        Self {
            textarea,
            messages,
            scroll_state: ScrollViewState::new(),
            init_scroll: true
        }
    }

    fn make_paragraph(msg: &DisplayMessage, width: u16) -> (Paragraph, u16) {
        let name_spans = vec![
            Span::styled(msg.get_time(), Style::default().fg(Color::DarkGray)),
            Span::from(" "),
            Span::styled(
                &msg.author,
                Style::default().fg(msg.get_message_color()).bold(),
            ),
            Span::from(">"),
        ];

        let name_str = name_spans
            .iter()
            .fold(String::new(), |total, span| total + span.content.as_ref());

        let msg_str = format!("{} {}", name_str, msg.content);

        let wrapped: Vec<String> = textwrap::wrap(&msg_str, width as usize)
            .into_iter()
            .map(|x| x.to_string())
            .collect();

        let height = wrapped.len() as u16;

        let mut wrapped = wrapped.into_iter();

        let first_line = wrapped.next().unwrap()[name_str.len()..].to_owned();

        let lines = std::iter::once(Line::from_iter(
            name_spans.into_iter().chain(std::iter::once(Span::raw(
                first_line
            ))),
        ))
        .chain(wrapped.map(|x| Line::from(x)));

        let paragraph = Paragraph::new(Text::from_iter(lines));

        (paragraph, height)
    }

    pub fn append(&mut self, msg: DisplayMessage) {
        self.messages.push(msg);
        self.reset_scroll();
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_state.scroll_to_bottom();
        self.scroll_state.scroll_page_up();
        self.scroll_state.scroll_down();
    }

    pub fn scroll_down(&mut self) {
        self.scroll_state.scroll_down();
    }

    pub fn scroll_up(&mut self) {
        self.scroll_state.scroll_up();
    }

    pub fn write_key(&mut self, key: PressedKey) {
        self.textarea.input(KeyEvent::from(key));
    }

    pub fn extract_input(&mut self) -> Option<String> {
        if self.textarea.is_empty() {
            None
        } else {
            self.textarea.delete_line_by_head();
            Some(self.textarea.yank_text())
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

// TODO: replace SendMsg with TextInput
// TODO: Return multiple AppActions from TextInput
// -> SendTextMessage
// -> SendImage

// TODO - implement multiple AppActions that can stem from TextInput
// but all of them will send data in a unified way to the EventManager

#[derive(Debug)]
pub enum MessageViewAction {
    ScrollUp,
    ScrollDown,
    //ReceiveMsg(String),
    TextInput(String),
    WriteKey(PressedKey),
}

impl<'a> Component for MessageView<'a> {
    type Action = MessageViewAction;
    type AppAction = action::AppAction;

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(self, area);
    }

    fn handle_kbd_event(&mut self, key: PressedKey) -> Option<Self::Action> {
        if key.code == KeyCode::Down {
            Some(Self::Action::ScrollDown)
        } else if key.code == KeyCode::Up {
            Some(Self::Action::ScrollUp)
        } else if key.code == KeyCode::Enter {
            self.extract_input()
                .map_or(None, |input| Some(Self::Action::TextInput(input)))
        } else {
            Some(Self::Action::WriteKey(key))
        }
    }

    fn react(&mut self, action: Self::Action) -> Result<Option<Self::AppAction>> {
        let result = match action {
            Self::Action::ScrollUp => {
                self.scroll_up();
                None
            },
            Self::Action::ScrollDown => {
                self.scroll_down();
                None
            },
            Self::Action::WriteKey(key) => {
                self.write_key(key);
                None
            }
            Self::Action::TextInput(input) => Some(Self::AppAction::ParseCommand(input))
        };

        Ok(result)
    }
}
