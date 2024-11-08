use chrono::{DateTime, Local, Utc};
use image::DynamicImage;
use libchatty::system::{FileMetadata, Hash};
use ratatui::{
    prelude::*,
    widgets::{Block, Paragraph},
};
use ratatui_image::{picker::Picker, protocol::Protocol, Image, Resize};
use std::marker::PhantomData;

use humansize::{format_size, DECIMAL};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug)]
pub enum MessageSide {
    Sender,
    Responder,
}

#[derive(Copy, Clone, Debug)]
pub enum TextStyle {
    Normal,
    Info
}

#[derive(Copy, Clone, Debug)]
pub struct MessageStyle {
    pub side: MessageSide,
    pub text: TextStyle
}

#[derive(Clone, Debug)]
pub enum Content {
    Text(String),
    File(FileMetadata),
}

#[derive(Clone, Debug)]
pub struct DisplayMessageMetadata {
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub style: MessageStyle,
}

impl DisplayMessageMetadata {
    pub fn get_time(&self) -> String {
        self.timestamp
            .with_timezone(&Local)
            .format("%H:%M:%S")
            .to_string()
    }

    pub fn get_user_color(&self) -> Color {
        match self.style.side {
            MessageSide::Sender => Color::Blue,
            MessageSide::Responder => Color::Green,
        }
    }

    pub fn get_text_color(&self) -> Color {
        match self.style.text {
            TextStyle::Info => Color::DarkGray,
            _ => Color::White,
        }
    }

    pub fn get_style(&self) -> Style {
        Style::default().bold()
    }
}

#[derive(Clone, Debug)]
pub struct DisplayMessage {
    pub meta: DisplayMessageMetadata,
    pub content: Content,
}

impl DisplayMessage {
    pub fn make_widget<'a>(
        &'a self,
        width: u16,
        imgdb: &'a HashMap<Hash, Box<dyn Protocol>>,
    ) -> DisplayMessageWidget<'a> {
        match &self.content {
            Content::Text(text) => DisplayMessageWidget::Text(
                ParagraphAutowidget::new(&self.meta, &text, width),
            ),
            Content::File(filemeta) => {
                let mut is_image = false;
                if let Some(mime) = &filemeta.filetype {
                    is_image = mime.type_() == mime::IMAGE;
                }

                if is_image && imgdb.contains_key(&filemeta.hash) {
                    return DisplayMessageWidget::Image(
                        ImageAutowidget::new(width, &self.meta, &filemeta, imgdb.get(&filemeta.hash).unwrap())
                    )
                }
                else {
                    return DisplayMessageWidget::File(FileAutowidget::new(
                        &self.meta, &filemeta, width,
                    ))
                }
            }
        }
    }
}

pub enum DisplayMessageWidget<'a> {
    Text(ParagraphAutowidget<'a>),
    File(FileAutowidget<'a>),
    Image(ImageAutowidget<'a>),
}

impl<'a> Autowidget for &DisplayMessageWidget<'a> {
    fn get_height(self) -> u16 {
        match self {
            DisplayMessageWidget::Text(widget) => widget.get_height(),
            DisplayMessageWidget::File(widget) => widget.get_height(),
            DisplayMessageWidget::Image(widget) => widget.get_height(),
        }
    }
}

impl<'a> Widget for &DisplayMessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self {
            DisplayMessageWidget::Text(widget) => widget.render(area, buf),
            DisplayMessageWidget::File(widget) => widget.render(area, buf),
            DisplayMessageWidget::Image(widget) => widget.render(area, buf),
        }
    }
}

pub trait Autowidget: Widget {
    fn get_height(self) -> u16;
}

pub struct ParagraphAutowidget<'a> {
    paragraph: Paragraph<'a>,
    height: u16,
}

impl<'a> ParagraphAutowidget<'a> {
    pub fn new(
        data: &'a DisplayMessageMetadata,
        content: &'a str,
        width: u16,
    ) -> Self {
        let name_spans = vec![
            Span::styled(data.get_time(), Style::default().fg(Color::DarkGray)),
            Span::from(" "),
            Span::styled(
                &data.author,
                data.get_style().fg(data.get_user_color()),
            ),
            Span::styled(">", Style::default().fg(data.get_user_color())),
        ];

        let name_str = name_spans
            .iter()
            .fold(String::new(), |total, span| total + span.content.as_ref());

        let msg_str = format!("{} {}", name_str, content);

        let wrapped: Vec<String> = textwrap::wrap(&msg_str, width as usize)
            .into_iter()
            .map(|x| x.to_string())
            .collect();

        let height = wrapped.len() as u16;

        let mut wrapped = wrapped.into_iter();

        let first_line = wrapped.next().unwrap()[name_str.len()..].to_owned();

        let lines = std::iter::once(Line::from_iter(
            name_spans
                .into_iter()
                .chain(std::iter::once(Span::raw(first_line))),
        ))
        .chain(wrapped.map(|x| Line::from(x)));

        let paragraph = Paragraph::new(
            Text::from_iter(lines)
                .style(Style::default().fg(data.get_text_color())),
        );

        Self { paragraph, height }
    }
}

impl<'a> Widget for &ParagraphAutowidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.paragraph.clone().render(area, buf);
    }
}

impl<'a> Autowidget for &ParagraphAutowidget<'a> {
    fn get_height(self) -> u16 {
        self.height
    }
}

pub struct FileAutowidget<'a> {
    data: &'a DisplayMessageMetadata,
    width: u16,
    text: String,
}

impl<'a> FileAutowidget<'a> {
    pub fn new(
        data: &'a DisplayMessageMetadata,
        content: &'a FileMetadata,
        width: u16,
    ) -> Self {
        Self {
            data,
            width,
            text: format!(
                "Sent file: {} ({})",
                content.name,
                format_size(content.size, DECIMAL)
            ),
        }
    }
}

impl<'a> Widget for &FileAutowidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let paragraph =
            ParagraphAutowidget::new(self.data, &self.text, self.width);
        paragraph.render(area, buf);
    }
}

impl<'a> Autowidget for &FileAutowidget<'a> {
    fn get_height(self) -> u16 {
        // TODO - Optimize this!
        let paragraph =
            ParagraphAutowidget::new(self.data, &self.text, self.width);
        paragraph.get_height()
    }
}

pub struct ImageAutowidget<'a> {
    pub paragraph: FileAutowidget<'a>,
    pub proto: &'a Box<dyn Protocol>,
}

impl<'a> ImageAutowidget<'a> {
    const HEIGHT: u16 = 12;

    pub fn new(
        width: u16,
        meta: &'a DisplayMessageMetadata,
        file_data: &'a FileMetadata,
        proto: &'a Box<dyn Protocol>,
    ) -> Self {
        Self {
            paragraph: FileAutowidget::new(meta, file_data, width),
            proto,
        }
    }
}

impl<'a> Widget for &ImageAutowidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [top, bottom] = Layout::default()
            .constraints([
                Constraint::Length(self.paragraph.get_height()),
                Constraint::Min(0),
            ])
            .areas(area);

        self.paragraph.render(top, buf);
        Image::new(&**self.proto).render(bottom, buf);
    }
}

impl<'a> Autowidget for &ImageAutowidget<'a> {
    fn get_height(self) -> u16 {
        self.paragraph.get_height() + ImageAutowidget::HEIGHT
    }
}
