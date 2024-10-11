use chrono::{DateTime, Local, Utc};
use std::fmt;

#[derive(Clone)]
pub struct DisplayMessage {
    pub content: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
}

impl DisplayMessage {
    fn get_time(&self) -> String {
        self.timestamp
            .with_timezone(&Local)
            .format("%H:%M:%S")
            .to_string()
    }
}

impl fmt::Display for DisplayMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}> {}", self.get_time(), self.author, self.content)
    }
}
