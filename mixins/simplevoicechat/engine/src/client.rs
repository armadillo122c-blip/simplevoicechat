//! The client half: the player's settings and keys, the microphone's frames out, the voices in,
//! and what the HUD shows.
use crate::{
    audio::{self, AudioIo},
    codec, dsp,
    menu::{self, Action, PeerRow},
    proto::{self, Cmd, Hello, Mic, Sound, SoundKind, State},
    spatial::{self, Listener},
};
use std::collections::{BTreeMap, HashMap};

/// The mod's keys, as Simple Voice Chat names them: name, label, default (as `native_mod_key`
/// reads them).
pub const KEYS: [[&str; 3]; 7] = [
    ["menu", "Voice chat menu", "key.keyboard.v"],
    ["group", "Voice chat groups", "key.keyboard.g"],
    ["push_to_talk", "Push to talk", "key.keyboard.caps.lock"],
    ["whisper", "Whisper (hold)", "key.keyboard.b"],
    ["mute", "Mute microphone", "key.keyboard.m"],
    ["disable", "Turn voice chat off", "key.keyboard.n"],
    ["hide", "Hide voice HUD", "key.keyboard.h"],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Menu,
    Group,
    PushToTalk,
    Whisper,
    Mute,
    Disable,
    Hide,
}

/// What a key press asks of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    None,
    /// Open the voice chat menu.
    OpenMenu,
    /// The settings changed: store them.
    Store,
}

impl Key {
    pub const ALL: [Key; 7] = [
        Key::Menu,
        Key::Group,
        Key::PushToTalk,
        Key::Whisper,
        Key::Mute,
        Key::Disable,
        Key::Hide,
    ];
    pub fn name(self) -> &'static str {
        KEYS[self as usize][0]
    }
    pub fn default_key(self) -> &'static str {
        KEYS[self as usize][2]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    PushToTalk,
    Voice,
}

const THRESHOLDS: [i32; 11] = [-70, -65, -60, -55, -50, -45, -40, -35, -30, -25, -20];
const AMPLIFICATIONS: [u32; 7] = [50, 75, 100, 125, 150, 200, 300];
const VOLUMES: [u32; 9] = [0, 25, 50, 75, 100, 125, 150, 175, 200];
const DISTANCES: [u32; 8] = [8, 16, 24, 32, 48, 64, 96, 128];
/// Frames voice activation stays open after the voice drops under the threshold (500 ms).
const HANGOVER: u32 = 25;
/// Frames sent a tick at most: older ones are dropped rather than lag.
const MAX_SEND: usize = 5;
/// Ticks a speaker counts as talking after their last frame.
const SPEAKING_TICKS: u32 = 8;
/// The id the mic test plays under.
const SELF: u128 = 0;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub activation: Activation,
    pub threshold_db: i32,
    pub amplification: u32,
    pub volume: u32,
    /// None: the system's default device.
    pub input: Option<String>,
    pub output: Option<String>,
    pub mic_test: bool,
    pub hud: bool,
    /// The voice distance of worlds this player hosts; whispers carry half.
    pub distance: u32,
    /// Each player's volume, in percent, set in the menu; 100 for the rest.
    pub volumes: BTreeMap<u128, u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            activation: Activation::PushToTalk,
            threshold_db: -50,
            amplification: 100,
            volume: 100,
            input: None,
            output: None,
            mic_test: false,
            hud: true,
            distance: 48,
            volumes: BTreeMap::new(),
        }
    }
}

/// The steps a player's volume moves by in the menu.
const PLAYER_STEP: u32 = 10;
const PLAYER_MAX: u32 = 200;

fn next_of<T: Copy + PartialEq>(list: &[T], v: T) -> T {
    let at = list.iter().position(|&x| x == v).unwrap_or(list.len() - 1);
    list[(at + 1) % list.len()]
}

fn next_device(list: &[String], v: &Option<String>) -> Option<String> {
    // Default, then each device, then Default again.
    let at = v.as_ref().and_then(|v| list.iter().position(|d| d == v));
    match at {
        None => list.first().cloned(),
        Some(i) => list.get(i + 1).cloned(),
    }
}

impl Settings {
    /// From the stored values (`get` reads one key's file); the defaults for the ones missing.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let num = |k: &str| get(k).and_then(|v| v.trim().parse::<i64>().ok());
        let flag = |k: &str, def: bool| get(k).map_or(def, |v| v.trim() == "true");
        let device = |k: &str| get(k).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        Self {
            activation: match get("activation").as_deref().map(str::trim) {
                Some("voice") => Activation::Voice,
                Some("ptt") => Activation::PushToTalk,
                _ => d.activation,
            },
            threshold_db: num("threshold").map_or(d.threshold_db, |v| v.clamp(-127, 0) as i32),
            amplification: num("amplification").map_or(d.amplification, |v| v.clamp(0, 1000) as u32),
            volume: num("volume").map_or(d.volume, |v| v.clamp(0, 1000) as u32),
            input: device("input"),
            output: device("output"),
            mic_test: flag("mic_test", d.mic_test),
            hud: flag("hud", d.hud),
            distance: num("distance").map_or(d.distance, |v| v.clamp(1, 512) as u32),
            // `uuid=percent;...`, the uuid in hex.
            volumes: get("player_volumes")
                .unwrap_or_default()
                .split(';')
                .filter_map(|e| {
                    let (u, v) = e.trim().split_once('=')?;
                    Some((u128::from_str_radix(u, 16).ok()?, v.parse::<u32>().ok()?.min(PLAYER_MAX)))
                })
                .collect(),
        }
    }

    /// A player's volume, in percent.
    pub fn player_volume(&self, uuid: u128) -> u32 {
        self.volumes.get(&uuid).copied().unwrap_or(100)
    }

    /// What to store, key by key.
    pub fn stored(&self) -> Vec<(&'static str, String)> {
        let flag = |b: bool| if b { "true" } else { "false" }.to_owned();
        vec![
            (
                "activation",
                match self.activation {
                    Activation::PushToTalk => "ptt",
                    Activation::Voice => "voice",
                }
                .to_owned(),
            ),
            ("threshold", self.threshold_db.to_string()),
            ("amplification", self.amplification.to_string()),
            ("volume", self.volume.to_string()),
            ("input", self.input.clone().unwrap_or_default()),
            ("output", self.output.clone().unwrap_or_default()),
            ("mic_test", flag(self.mic_test)),
            ("hud", flag(self.hud)),
            ("distance", self.distance.to_string()),
            (
                "player_volumes",
                self.volumes
                    .iter()
                    .map(|(u, v)| format!("{u:032x}={v}"))
                    .collect::<Vec<_>>()
                    .join(";"),
            ),
        ]
    }

    /// The settings page's buttons.
    pub fn labels(&self) -> Vec<String> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        let device = |d: &Option<String>| d.clone().unwrap_or_else(|| "Default".into());
        vec![
            format!(
                "Activation: {}",
                match self.activation {
                    Activation::PushToTalk => "Push to talk",
                    Activation::Voice => "Voice",
                }
            ),
            format!("Voice activation threshold: {} dB", self.threshold_db),
            format!("Microphone amplification: {}%", self.amplification),
            format!("Voice chat volume: {}%", self.volume),
            format!("Microphone: {}", device(&self.input)),
            format!("Speakers: {}", device(&self.output)),
            format!("Hear yourself (mic test): {}", on(self.mic_test)),
            format!("Voice HUD: {}", on(self.hud)),
            format!("Voice distance when you host: {} blocks", self.distance),
        ]
    }

    /// A click on button `i` of `labels`; `inputs` and `outputs` list the devices. True when
    /// the devices changed and need opening again.
    pub fn click(&mut self, i: usize, inputs: &[String], outputs: &[String]) -> bool {
        match i {
            0 => {
                self.activation = match self.activation {
                    Activation::PushToTalk => Activation::Voice,
                    Activation::Voice => Activation::PushToTalk,
                }
            }
            1 => self.threshold_db = next_of(&THRESHOLDS, self.threshold_db),
            2 => self.amplification = next_of(&AMPLIFICATIONS, self.amplification),
            3 => self.volume = next_of(&VOLUMES, self.volume),
            4 => {
                self.input = next_device(inputs, &self.input);
                return true;
            }
            5 => {
                self.output = next_device(outputs, &self.output);
                return true;
            }
            6 => self.mic_test = !self.mic_test,
            7 => self.hud = !self.hud,
            8 => self.distance = next_of(&DISTANCES, self.distance),
            _ => {}
        }
        false
    }
}

/// A line of the HUD: text and colour (0xRRGGBB).
pub type HudLine = (String, u32);

#[derive(Default)]
pub struct VoiceClient {
    settings: Option<Settings>,
    audio: Option<AudioIo>,
    devices: Option<(Vec<String>, Vec<String>)>,
    in_world: bool,
    ptt: bool,
    whisper: bool,
    muted: bool,
    disabled: bool,
    hangover: u32,
    talking: bool,
    level_db: f32,
    seq: u32,
    /// The server answered: it has the mod too.
    connected: bool,
    hello_due: bool,
    ticks: u32,
    state: Option<State>,
    /// Who was heard last, by tick.
    speaking: HashMap<u128, u32>,
    /// The menu's text box and whether it has the keys.
    field: String,
    field_focus: bool,
    /// Group actions from the menu, sent on the next tick.
    pending: Vec<String>,
    /// The menu key was pressed on the menu: the next tick closes it.
    close_menu: bool,
}

/// What a key press on the open menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    /// Not the menu's: the screen handles it.
    Pass,
    /// The menu used it.
    Taken,
    /// The menu key again: close the menu.
    Close,
}

impl VoiceClient {
    /// The settings, read once through `get`.
    pub fn settings(&mut self, get: impl Fn(&str) -> Option<String>) -> &Settings {
        self.settings.get_or_insert_with(|| Settings::read(get))
    }

    /// The device names for the settings page, listed once.
    pub fn devices(&mut self) -> (Vec<String>, Vec<String>) {
        self.devices
            .get_or_insert_with(|| (audio::input_devices(), audio::output_devices()))
            .clone()
    }

    /// The settings page changed them; `reopen` when the devices changed.
    pub fn set_settings(&mut self, settings: Settings, reopen: bool) {
        if !settings.mic_test && let Some(a) = &self.audio {
            a.silence();
        }
        self.settings = Some(settings);
        if reopen {
            self.audio = None;
        }
    }

    /// The player pressed or let go of one of the mod's keys; `playing` with no screen open.
    pub fn key(&mut self, key: Key, pressed: bool, playing: bool) -> KeyOutcome {
        if pressed && !playing {
            return KeyOutcome::None;
        }
        match key {
            Key::PushToTalk => self.ptt = pressed,
            Key::Whisper => self.whisper = pressed,
            Key::Menu | Key::Group if pressed => {
                self.field_focus = false;
                return KeyOutcome::OpenMenu;
            }
            Key::Mute if pressed => self.toggle_mute(),
            Key::Disable if pressed => self.toggle_disabled(),
            Key::Hide if pressed => {
                let s = self.settings.get_or_insert_with(Settings::default);
                s.hud = !s.hud;
                return KeyOutcome::Store;
            }
            _ => {}
        }
        KeyOutcome::None
    }

    fn toggle_mute(&mut self) {
        self.muted = !self.muted;
    }

    fn toggle_disabled(&mut self) {
        self.disabled = !self.disabled;
        self.hello_due = true;
        if self.disabled
            && let Some(a) = &self.audio
        {
            a.silence();
        }
    }

    /// The menu's contents now.
    pub fn menu_view(&self) -> menu::View {
        let s = self.settings.clone().unwrap_or_default();
        let state = self.state.as_ref();
        let peers = state.map_or_else(Vec::new, |st| {
            st.peers
                .iter()
                .map(|p| PeerRow {
                    uuid: p.uuid,
                    name: p.name.clone(),
                    volume: s.player_volume(p.uuid),
                    speaking: self.speaking.contains_key(&p.uuid),
                    grouped: p.grouped,
                    disabled: p.disabled,
                })
                .collect()
        });
        menu::View {
            connected: self.connected,
            muted: self.muted,
            disabled: self.disabled,
            hud: s.hud,
            voice_activation: s.activation == Activation::Voice,
            mic_test: s.mic_test,
            peers,
            group: state.and_then(|st| st.group.clone().map(|g| (g, st.isolated))),
            groups: state.map_or_else(Vec::new, |st| st.groups.clone()),
            field: self.field.clone(),
            field_focus: self.field_focus,
        }
    }

    /// A click on the menu. True when the settings changed and need storing.
    pub fn menu_action(&mut self, action: Action) -> bool {
        self.field_focus = action == Action::Field;
        let s = self.settings.get_or_insert_with(Settings::default);
        match action {
            Action::Mute => self.toggle_mute(),
            Action::Disable => self.toggle_disabled(),
            Action::Hud => {
                s.hud = !s.hud;
                return true;
            }
            Action::Activation => {
                s.activation = match s.activation {
                    Activation::PushToTalk => Activation::Voice,
                    Activation::Voice => Activation::PushToTalk,
                };
                return true;
            }
            Action::MicTest => {
                s.mic_test = !s.mic_test;
                if !s.mic_test
                    && let Some(a) = &self.audio
                {
                    a.silence();
                }
                return true;
            }
            Action::Volume(uuid, dir) => {
                let v = s.player_volume(uuid);
                let v = if dir < 0 {
                    v.saturating_sub(PLAYER_STEP)
                } else {
                    (v + PLAYER_STEP).min(PLAYER_MAX)
                };
                if v == 100 {
                    s.volumes.remove(&uuid);
                } else {
                    s.volumes.insert(uuid, v);
                }
                return true;
            }
            Action::Join(group) => {
                // A password in the box goes with it.
                let pw = self.field.trim();
                let line = if pw.is_empty() {
                    format!("join {group}")
                } else {
                    format!("join {group} {}", pw.split_whitespace().next().unwrap_or(""))
                };
                self.pending.push(line);
                self.field.clear();
            }
            Action::Create => {
                let mut words = self.field.split_whitespace();
                if let Some(name) = words.next() {
                    let line = match words.next() {
                        Some(pw) => format!("join {name} {pw}"),
                        None => format!("join {name}"),
                    };
                    self.pending.push(line);
                }
                self.field.clear();
            }
            Action::Leave => self.pending.push("leave".into()),
            Action::Isolate => self.pending.push("isolate".into()),
            Action::Field => {}
        }
        false
    }

    /// A character typed on the menu: into the text box when it has the keys.
    pub fn menu_char(&mut self, c: char) -> bool {
        if !self.field_focus {
            return false;
        }
        // Group names are command words: letters, digits, `_` and `-`; a space starts the
        // password.
        if (c.is_ascii_alphanumeric() || "_- ".contains(c)) && self.field.len() < 32 {
            self.field.push(c);
        }
        true
    }

    /// A key pressed on the menu (`key.keyboard.backspace`...); `menu_keys` are the keys bound
    /// to the menu, which close it unless the text box has the keys.
    pub fn menu_key(&mut self, key: &str, menu_keys: &[String]) -> MenuKey {
        if self.field_focus {
            match key.rsplit('.').next().unwrap_or(key) {
                "backspace" => {
                    self.field.pop();
                }
                "enter" => {
                    self.menu_action(Action::Create);
                }
                "escape" => return MenuKey::Pass,
                _ => {}
            }
            // Everything else is typing, never the screen's own key (E, the menu key...).
            return MenuKey::Taken;
        }
        if menu_keys.iter().any(|k| k == key) {
            self.close_menu = true;
            return MenuKey::Close;
        }
        MenuKey::Pass
    }

    /// A click on the menu but on nothing: the text box lets go of the keys.
    pub fn menu_blur(&mut self) {
        self.field_focus = false;
    }

    /// Whether the menu should close now (the screen hooks can't close it themselves).
    pub fn take_close(&mut self) -> bool {
        std::mem::take(&mut self.close_menu)
    }

    /// Once a client tick. `listener` is the player, None outside a world. Returns the
    /// messages for the server.
    pub fn tick(&mut self, listener: Option<Listener>) -> Vec<(&'static str, Vec<u8>)> {
        self.ticks = self.ticks.wrapping_add(1);
        let mut out = Vec::new();
        if listener.is_none() {
            if self.in_world {
                // Left the world: let the devices go and forget the server.
                self.in_world = false;
                self.audio = None;
                self.connected = false;
                self.state = None;
                self.speaking.clear();
                self.ptt = false;
                self.whisper = false;
            }
            return out;
        }
        if !self.in_world {
            self.in_world = true;
            self.hello_due = true;
        }
        let settings = self.settings.clone().unwrap_or_default();
        // Opened on entering a world and when the settings page picks other devices.
        if self.audio.is_none() {
            self.audio = Some(AudioIo::open(settings.input.as_deref(), settings.output.as_deref()));
        }
        // Say hello until the server answers, and whenever voice chat is turned on or off.
        if self.hello_due || (!self.connected && self.ticks % 40 == 0) {
            self.hello_due = false;
            out.push((proto::HELLO, Hello { disabled: self.disabled }.encode()));
        }
        if self.connected {
            for line in self.pending.drain(..) {
                out.push((proto::CMD, Cmd { line }.encode()));
            }
        }
        let Some(audio) = &self.audio else {
            return out;
        };
        let mut frames = audio.take_frames();
        let late = frames.len().saturating_sub(MAX_SEND);
        frames.drain(..late);
        let gain = settings.amplification as f32 / 100.0;
        let mut sent = Vec::new();
        let mut active_any = false;
        for mut f in frames {
            dsp::amplify(&mut f, gain);
            let level = dsp::level_db(&f);
            self.level_db = level;
            if level > settings.threshold_db as f32 {
                self.hangover = HANGOVER;
            } else {
                self.hangover = self.hangover.saturating_sub(1);
            }
            let active = !self.muted
                && !self.disabled
                && match settings.activation {
                    Activation::PushToTalk => self.ptt,
                    Activation::Voice => self.hangover > 0,
                };
            if !active {
                continue;
            }
            active_any = true;
            let encoded = codec::encode(&f);
            if settings.mic_test
                && let Some(pcm) = codec::decode(&encoded)
            {
                let v = settings.volume as f32 / 100.0;
                audio.play(SELF, &pcm, v, v);
            }
            sent.push(encoded);
        }
        self.talking = active_any || (self.talking && self.ptt && !self.muted && !self.disabled);
        if !sent.is_empty() && self.connected {
            self.seq = self.seq.wrapping_add(1);
            let mic = Mic {
                whisper: self.whisper,
                seq: self.seq,
                frames: sent,
            };
            out.push((proto::MIC, mic.encode()));
        }
        let now = self.ticks;
        self.speaking.retain(|_, t| now.wrapping_sub(*t) <= SPEAKING_TICKS);
        out
    }

    /// A message from the server; `listener` is the player as they stand now.
    pub fn message(&mut self, channel: &str, data: &[u8], listener: Option<Listener>) {
        match channel {
            proto::STATE => {
                if let Some(s) = State::decode(data) {
                    self.connected = true;
                    self.state = Some(s);
                }
            }
            proto::SOUND => {
                let Some(sound) = Sound::decode(data) else {
                    return;
                };
                self.connected = true;
                if self.disabled {
                    return;
                }
                let Some(audio) = &self.audio else {
                    return;
                };
                let volume = self.settings.as_ref().map_or(1.0, |s| {
                    s.volume as f32 / 100.0 * s.player_volume(sound.sender) as f32 / 100.0
                });
                let (left, right) = match (sound.kind, listener) {
                    (SoundKind::Group, _) => (1.0, 1.0),
                    (SoundKind::Proximity { pos, range }, Some(l)) => spatial::gains(&l, pos, range),
                    (SoundKind::Proximity { .. }, None) => return,
                };
                for f in &sound.frames {
                    if let Some(pcm) = codec::decode(f) {
                        audio.play(sound.sender, &pcm, left * volume, right * volume);
                    }
                }
                self.speaking.insert(sound.sender, self.ticks);
            }
            _ => {}
        }
    }

    /// What the HUD shows, top line first; empty when the HUD is off or outside a world.
    pub fn hud(&self) -> Vec<HudLine> {
        const WHITE: u32 = 0xFFFFFF;
        const GREEN: u32 = 0x55FF55;
        const RED: u32 = 0xFF5555;
        const GRAY: u32 = 0xAAAAAA;
        const YELLOW: u32 = 0xFFFF55;
        if !self.in_world || !self.settings.as_ref().is_none_or(|s| s.hud) {
            return Vec::new();
        }
        let mut lines = Vec::new();
        let ptt = self.settings.as_ref().is_none_or(|s| s.activation == Activation::PushToTalk);
        let status = if self.disabled {
            ("Voice: off".to_owned(), RED)
        } else if self.audio.as_ref().is_some_and(|a| !a.has_mic()) {
            let why = self.audio.as_ref().and_then(|a| a.problem.clone()).unwrap_or_default();
            (format!("Voice: no microphone ({why})"), RED)
        } else if self.muted {
            ("Voice: muted".to_owned(), RED)
        } else if self.talking {
            let how = if self.whisper { "whispering" } else { "talking" };
            (format!("Voice: {how}"), GREEN)
        } else if ptt {
            ("Voice: push to talk".to_owned(), GRAY)
        } else {
            ("Voice: listening".to_owned(), GRAY)
        };
        lines.push(status);
        if !self.connected {
            lines.push(("Server has no voice chat".into(), GRAY));
            return lines;
        }
        if let Some(state) = &self.state {
            if let Some(g) = &state.group {
                let iso = if state.isolated { " (isolated)" } else { "" };
                lines.push((format!("Group: {g}{iso}"), YELLOW));
                for p in state.peers.iter().filter(|p| p.grouped) {
                    let colour = if self.speaking.contains_key(&p.uuid) { GREEN } else { WHITE };
                    lines.push((format!("  {}", p.name), colour));
                }
            }
            for p in &state.peers {
                if self.speaking.contains_key(&p.uuid) && !p.grouped {
                    lines.push((format!("> {}", p.name), GREEN));
                }
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |key| pairs.iter().find(|p| p.0 == key).map(|p| p.1.to_owned())
    }

    #[test]
    fn settings_read_store_and_cycle() {
        assert_eq!(Settings::read(stored(&[])), Settings::default());
        let mut s = Settings::read(stored(&[
            ("activation", "voice"),
            ("threshold", "-35\n"),
            ("input", "USB Mic"),
            ("hud", "false"),
        ]));
        assert_eq!(s.activation, Activation::Voice);
        assert_eq!(s.threshold_db, -35);
        assert_eq!(s.input.as_deref(), Some("USB Mic"));
        assert!(!s.hud);
        let pairs = s.stored();
        let back: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(Settings::read(stored(&back)), s);

        s.click(1, &[], &[]);
        assert_eq!(s.threshold_db, -30);
        s.threshold_db = -20;
        s.click(1, &[], &[]);
        assert_eq!(s.threshold_db, -70);
        let mics = ["A".to_owned(), "B".to_owned()];
        s.input = None;
        assert!(s.click(4, &mics, &[]));
        assert_eq!(s.input.as_deref(), Some("A"));
        s.click(4, &mics, &[]);
        s.click(4, &mics, &[]);
        assert_eq!(s.input, None, "back to Default");
        assert_eq!(s.labels().len(), 9);
    }

    #[test]
    fn keys_toggle_and_hold() {
        let mut c = VoiceClient::default();
        c.key(Key::Mute, true, false);
        assert!(!c.muted, "not while a screen is open");
        c.key(Key::Mute, true, true);
        assert!(c.muted);
        assert_eq!(c.key(Key::Menu, true, true), KeyOutcome::OpenMenu);
        assert_eq!(c.key(Key::Group, true, true), KeyOutcome::OpenMenu);
        assert_eq!(c.key(Key::Menu, true, false), KeyOutcome::None);
        assert_eq!(c.key(Key::Hide, true, true), KeyOutcome::Store);
        assert!(!c.settings.as_ref().unwrap().hud);
        c.key(Key::Mute, false, true);
        assert!(c.muted, "a toggle, not a hold");
        c.key(Key::PushToTalk, true, true);
        assert!(c.ptt);
        c.key(Key::PushToTalk, false, false);
        assert!(!c.ptt, "letting go counts anywhere");
        c.key(Key::Disable, true, true);
        assert!(c.disabled && c.hello_due);
    }

    #[test]
    fn menu_actions_volumes_and_typing() {
        let mut c = VoiceClient {
            connected: true,
            ..Default::default()
        };
        // Player volumes step by 10 and are stored by uuid.
        assert!(c.menu_action(Action::Volume(0xAB, 1)));
        assert!(c.menu_action(Action::Volume(0xAB, 1)));
        let s = c.settings.clone().unwrap();
        assert_eq!(s.player_volume(0xAB), 120);
        assert_eq!(s.player_volume(0xCD), 100);
        let pairs = s.stored();
        let back: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(Settings::read(stored(&back)).player_volume(0xAB), 120);
        for _ in 0..30 {
            c.menu_action(Action::Volume(0xAB, -1));
        }
        assert_eq!(c.settings.as_ref().unwrap().player_volume(0xAB), 0);

        // Typing goes into the box only once it has the keys; the menu key closes otherwise.
        let keys = ["key.keyboard.v".to_owned()];
        assert!(!c.menu_char('x'));
        assert_eq!(c.menu_key("key.keyboard.v", &keys), MenuKey::Close);
        c.menu_action(Action::Field);
        for ch in "miners!pw".chars() {
            c.menu_char(ch);
        }
        assert_eq!(c.field, "minerspw");
        c.menu_key("key.keyboard.backspace", &keys);
        c.menu_key("key.keyboard.backspace", &keys);
        c.menu_char(' ');
        c.menu_char('7');
        assert_eq!(c.menu_key("key.keyboard.v", &keys), MenuKey::Taken, "typing, not closing");
        assert_eq!(c.menu_key("key.keyboard.enter", &keys), MenuKey::Taken);
        assert_eq!(c.pending, ["join miners 7"]);
        assert!(c.field.is_empty());

        c.menu_action(Action::Leave);
        c.menu_action(Action::Join("team".into()));
        let out = c.tick(None);
        assert!(out.is_empty(), "outside a world nothing goes");
        c.in_world = true;
        let sent: Vec<String> = c
            .pending
            .clone()
            .into_iter()
            .collect();
        assert_eq!(sent, ["join miners 7", "leave", "join team"]);
    }

    #[test]
    fn hud_lines() {
        let mut c = VoiceClient {
            in_world: true,
            connected: true,
            ..Default::default()
        };
        c.state = Some(State {
            group: Some("team".into()),
            isolated: false,
            distance: 48.0,
            peers: vec![
                proto::Peer {
                    uuid: 5,
                    name: "Steve".into(),
                    disabled: false,
                    grouped: true,
                },
                proto::Peer {
                    uuid: 6,
                    name: "Alex".into(),
                    disabled: false,
                    grouped: false,
                },
            ],
            groups: Vec::new(),
        });
        c.speaking.insert(6, 0);
        let hud = c.hud();
        assert_eq!(hud[0].0, "Voice: push to talk");
        assert_eq!(hud[1].0, "Group: team");
        assert_eq!(hud[2].0, "  Steve");
        assert_eq!(hud[3], ("> Alex".into(), 0x55FF55));
        c.disabled = true;
        assert_eq!(c.hud()[0].0, "Voice: off");
    }
}
