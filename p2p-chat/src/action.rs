use std::path::PathBuf;

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
    SendPeerMessage(PeerMessageData, VerifyingKey),
    SendTextMessage(String),
    SendImageMessage(PathBuf),
    SendFileMessage(PathBuf),
    SetOffline,
    SetConnecting,
    SetConnected,
}
