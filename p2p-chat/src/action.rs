use std::path::PathBuf;

use crate::tui::TuiAction;
use libchatty::{
    messaging::{PeerMessageData, UserMessage},
    system::FileMetadata
};
use ed25519_dalek::VerifyingKey;

pub enum AppAction {
    Quit,
    Redraw,
    TuiAction(TuiAction),
    SelectUser(VerifyingKey),
    ReceiveMessage(UserMessage),
    DownloadFile,
    ReceiveDownloadedFile,
    ParseCommand(String),
    SendPeerMessage(PeerMessageData, VerifyingKey),
    SendTextMessage(String),
    ShareFile(PathBuf),
    SetOffline,
    SetConnecting,
    SetConnected,
}
