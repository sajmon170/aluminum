use chrono::{DateTime, Local, Utc};
use ratatui::style::Color;

#[derive(Copy, Clone, Debug)]
pub enum MessageStyle {
    Sender,
    Responder
}

// NOTE: We can't implement a Widget trait for the DisplayMessage since
// the callee doesn't know its Rect bounding box in advance.
// We can implement a get_bounding_box() function on this struct but it would
// require calculating line wrapping. We would then have to calculate the line
// wrapping all over inside the render() function. We can't cache this wrapping
// since such an operation would create a self-referencing struct. 

#[derive(Clone, Debug)]
pub struct DisplayMessage {
    pub content: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub style: MessageStyle
}

impl DisplayMessage {
    pub fn get_time(&self) -> String {
        self.timestamp
            .with_timezone(&Local)
            .format("%H:%M:%S")
            .to_string()
    }

    pub fn get_message_color(&self) -> Color {
        match self.style {
            MessageStyle::Sender => Color::Blue,
            MessageStyle::Responder => Color::Green
        }
    }
}
