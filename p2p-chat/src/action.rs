use crate::tui::TuiAction;
use libchatty::messaging::{PeerMessageData, UserMessage};
use ed25519_dalek::VerifyingKey;

pub enum AppAction {
    Quit,
    Redraw,
    TuiAction(TuiAction),
    SelectUser(VerifyingKey),
    ReceiveMessage(UserMessage),
    ParseCommand(String),
    SendTextMessage(PeerMessageData, VerifyingKey),
    SetOffline,
    SetConnecting,
    SetConnected,
}
