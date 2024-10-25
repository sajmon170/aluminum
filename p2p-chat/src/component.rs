use crate::eventmanager::PressedKey;
pub use ratatui::prelude::*;
use color_eyre::Result;

pub trait Component {
    type Action;
    type AppAction;
    fn draw(&mut self, frame: &mut Frame, area: Rect);
    fn handle_kbd_event(&mut self, key: PressedKey) -> Option<Self::Action>;
    fn react(&mut self, action: Self::Action) -> Result<Option<Self::AppAction>>;
}
