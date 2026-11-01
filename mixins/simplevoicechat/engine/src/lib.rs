//! Simple Voice Chat for Horizon: proximity voice between players, as henkelmax's Simple Voice
//! Chat does for Java Edition, rebuilt in Rust on Horizon's native hook points.
//!
//! The mod has two halves. The client half (`client`) records the microphone, decides when the
//! player is talking (push to talk or voice activation), encodes 20 ms frames (`codec`) and sends
//! them to the server; it plays what the server sends back, panned and faded by where the speaker
//! stands (`spatial`, `audio`). The server half (`server`) knows every player's position and
//! group, and forwards each frame to the players who should hear it. Both halves talk over
//! Horizon's mod messages (`horizon:mod_net`), in the formats of `proto`.
//!
//! Each half keeps its state per thread: the window's game and the server thread's game are two
//! threads, so a single player world keeps them apart on its own.
pub mod audio;
pub mod client;
pub mod codec;
pub mod dsp;
pub mod proto;
pub mod server;
pub mod spatial;

use std::cell::RefCell;

/// The mod's id: its settings folder, its keys' lines and its channels' namespace.
pub const ID: &str = "simplevoicechat";
pub const VERSION: &str = "0.1.0";
pub const TITLE: &str = "Simple Voice Chat";
pub const DESCRIPTION: &str = "Proximity voice chat: talk to the players near you, or to your group anywhere. \
     Push to talk or voice activation. A Horizon port of henkelmax's Simple Voice Chat.";

/// Everything runs at 48 kHz mono in 20 ms frames, as Simple Voice Chat does.
pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME: usize = 960;

thread_local! {
    static CLIENT: RefCell<client::VoiceClient> = RefCell::new(client::VoiceClient::default());
    static SERVER: RefCell<server::VoiceServer> = RefCell::new(server::VoiceServer::default());
}

/// This thread's client half.
pub fn with_client<R>(f: impl FnOnce(&mut client::VoiceClient) -> R) -> R {
    CLIENT.with_borrow_mut(f)
}

/// This thread's server half.
pub fn with_server<R>(f: impl FnOnce(&mut server::VoiceServer) -> R) -> R {
    SERVER.with_borrow_mut(f)
}
