pub use ratatui::prelude::*;
use crate::eventmanager::PressedKey;

pub trait Component {
    type Action;
    fn draw(&mut self, frame: &mut Frame, area: Rect);
    fn handle_kbd_event(&mut self, key: PressedKey) -> Option<Self::Action>;
    fn react(&mut self, action: Self::Action) -> std::io::Result<()>;
}
