//! The server half: who has voice chat, where everyone stands, the groups, and which players
//! each frame goes to.
//!
//! Routing follows Simple Voice Chat: a voice reaches every player in the same dimension within
//! the voice distance (half of it while whispering), and every member of the speaker's group
//! wherever they are. An isolated group's members hear only each other, and nobody outside
//! hears them.
use crate::proto::{self, Hello, Mic, Peer, Sound, SoundKind, State};
use std::collections::HashMap;

/// A player online, as the server tick sees them.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub uuid: u128,
    pub name: String,
    pub dimension: String,
    /// Their feet.
    pub position: [f64; 3],
}

/// Where a voice comes from: a standing player's eyes.
const EYES: f64 = 1.62;
/// Ticks between state messages when nothing changed.
const STATE_EVERY: u64 = 40;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// Blocks a voice carries.
    pub distance: f32,
    /// Blocks a whisper carries.
    pub whisper: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            distance: 48.0,
            whisper: 24.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Client {
    disabled: bool,
}

#[derive(Clone, Debug)]
struct Group {
    name: String,
    password: Option<String>,
    isolated: bool,
    members: Vec<u128>,
}

#[derive(Default)]
pub struct VoiceServer {
    pub config: Config,
    clients: HashMap<u128, Client>,
    players: Vec<Player>,
    groups: Vec<Group>,
    out: Vec<(u128, &'static str, Vec<u8>)>,
    dirty: bool,
    ticks: u64,
}

impl VoiceServer {
    /// Forgets everything: the world closed.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The end of a server tick, with the players online: the ones who left are forgotten, and
    /// everyone with the mod hears who is there now and then.
    pub fn tick(&mut self, players: Vec<Player>) {
        self.ticks += 1;
        let online = |u: &u128| players.iter().any(|p| p.uuid == *u);
        let before = self.clients.len();
        self.clients.retain(|u, _| online(u));
        for g in &mut self.groups {
            g.members.retain(online);
        }
        self.groups.retain(|g| !g.members.is_empty());
        let joined = players.iter().map(|p| (p.uuid, &p.name));
        let names_changed = self.players.len() != players.len()
            || self.players.iter().zip(joined).any(|(a, b)| (a.uuid, &a.name) != b);
        self.dirty |= before != self.clients.len() || names_changed;
        self.players = players;
        if self.dirty || self.ticks % STATE_EVERY == 0 {
            self.dirty = false;
            self.send_states();
        }
    }

    /// A message from `from`'s client.
    pub fn message(&mut self, from: u128, channel: &str, data: &[u8]) {
        match channel {
            proto::HELLO => {
                if let Some(h) = Hello::decode(data) {
                    let was = self.clients.insert(from, Client { disabled: h.disabled });
                    if was.is_none_or(|c| c.disabled != h.disabled) {
                        self.dirty = true;
                    }
                }
            }
            proto::MIC => {
                if self.clients.get(&from).is_some_and(|c| !c.disabled)
                    && let Some(mic) = Mic::decode(data)
                {
                    self.route(from, mic);
                }
            }
            _ => {}
        }
    }

    /// What to send: (player, channel, data).
    pub fn drain(&mut self) -> Vec<(u128, &'static str, Vec<u8>)> {
        std::mem::take(&mut self.out)
    }

    fn group_of(&self, uuid: u128) -> Option<usize> {
        self.groups.iter().position(|g| g.members.contains(&uuid))
    }

    fn route(&mut self, from: u128, mic: Mic) {
        if mic.frames.is_empty() {
            return;
        }
        let Some(speaker) = self.players.iter().find(|p| p.uuid == from) else {
            return;
        };
        let speaker_group = self.group_of(from);
        let speaker_isolated = speaker_group.is_some_and(|g| self.groups[g].isolated);
        let range = if mic.whisper {
            self.config.whisper
        } else {
            self.config.distance
        };
        let eyes = [
            speaker.position[0],
            speaker.position[1] + EYES,
            speaker.position[2],
        ];
        let mut out = Vec::new();
        for p in &self.players {
            if p.uuid == from || !self.clients.get(&p.uuid).is_some_and(|c| !c.disabled) {
                continue;
            }
            let group = self.group_of(p.uuid);
            let kind = if speaker_group.is_some() && group == speaker_group {
                SoundKind::Group
            } else {
                let isolated = group.is_some_and(|g| self.groups[g].isolated);
                if speaker_isolated || isolated || p.dimension != speaker.dimension {
                    continue;
                }
                let d: f64 = (0..3)
                    .map(|i| (p.position[i] - speaker.position[i]).powi(2))
                    .sum();
                if d.sqrt() > f64::from(range) {
                    continue;
                }
                SoundKind::Proximity { pos: eyes, range }
            };
            let sound = Sound {
                sender: from,
                kind,
                whisper: mic.whisper,
                seq: mic.seq,
                frames: mic.frames.clone(),
            };
            out.push((p.uuid, proto::SOUND, sound.encode()));
        }
        self.out.extend(out);
    }

    fn send_states(&mut self) {
        let mut out = Vec::new();
        for (&uuid, _) in &self.clients {
            let group = self.group_of(uuid);
            let peers = self
                .players
                .iter()
                .filter(|p| p.uuid != uuid)
                .filter_map(|p| {
                    let c = self.clients.get(&p.uuid)?;
                    Some(Peer {
                        uuid: p.uuid,
                        name: p.name.clone(),
                        disabled: c.disabled,
                        grouped: group.is_some() && self.group_of(p.uuid) == group,
                    })
                })
                .collect();
            let state = State {
                group: group.map(|g| self.groups[g].name.clone()),
                isolated: group.is_some_and(|g| self.groups[g].isolated),
                distance: self.config.distance,
                peers,
            };
            out.push((uuid, proto::STATE, state.encode()));
        }
        self.out.extend(out);
    }

    /// The usages `/voicechat` registers.
    pub const COMMANDS: [&'static str; 6] = [
        "voicechat help",
        "voicechat join <group:word> [password:word]",
        "voicechat leave",
        "voicechat groups",
        "voicechat isolate",
        "voicechat invite <player:player>",
    ];

    /// `player`'s `/voicechat ...` (the whole line, without its slash): the lines to answer
    /// with, and lines for other players.
    pub fn command(&mut self, player: u128, line: &str) -> (Vec<String>, Vec<(u128, String)>) {
        let mut words = line.split_whitespace();
        if words.next() != Some("voicechat") {
            return (Vec::new(), Vec::new());
        }
        let name = |s: &Self, u: u128| {
            s.players
                .iter()
                .find(|p| p.uuid == u)
                .map_or_else(|| "someone".to_owned(), |p| p.name.clone())
        };
        let mut others = Vec::new();
        let reply = match words.next().unwrap_or("help") {
            "join" => {
                let Some(group) = words.next() else {
                    return (vec!["Usage: /voicechat join <group> [password]".into()], others);
                };
                let password = words.next().map(str::to_owned);
                if self.groups.get(self.group_of(player).unwrap_or(usize::MAX)).is_some_and(|g| g.name == group) {
                    vec![format!("You are already in {group}.")]
                } else if let Some(i) = self.groups.iter().position(|g| g.name == group) {
                    if self.groups[i].password.is_some() && self.groups[i].password != password {
                        vec![format!("Wrong password for {group}.")]
                    } else {
                        self.leave(player, &mut others);
                        let i = self.groups.iter().position(|g| g.name == group).expect("still there");
                        let who = name(self, player);
                        for &m in &self.groups[i].members {
                            others.push((m, format!("{who} joined the voice group.")));
                        }
                        self.groups[i].members.push(player);
                        self.dirty = true;
                        vec![format!("You joined the voice group {group}.")]
                    }
                } else {
                    self.leave(player, &mut others);
                    self.groups.push(Group {
                        name: group.to_owned(),
                        password: password.clone(),
                        isolated: false,
                        members: vec![player],
                    });
                    self.dirty = true;
                    let lock = if password.is_some() { " with a password" } else { "" };
                    vec![format!(
                        "You made the voice group {group}{lock}. Others join with /voicechat join {group}."
                    )]
                }
            }
            "leave" => {
                if self.leave(player, &mut others) {
                    vec!["You left your voice group.".into()]
                } else {
                    vec!["You are not in a voice group.".into()]
                }
            }
            "groups" => {
                if self.groups.is_empty() {
                    vec!["There are no voice groups. Make one with /voicechat join <group>.".into()]
                } else {
                    let mut lines = vec!["Voice groups:".to_owned()];
                    for g in &self.groups {
                        let members: Vec<String> = g.members.iter().map(|&m| name(self, m)).collect();
                        let mut tags = Vec::new();
                        if g.password.is_some() {
                            tags.push("password");
                        }
                        if g.isolated {
                            tags.push("isolated");
                        }
                        let tags = if tags.is_empty() { String::new() } else { format!(" ({})", tags.join(", ")) };
                        lines.push(format!("- {}{tags}: {}", g.name, members.join(", ")));
                    }
                    lines
                }
            }
            "isolate" => match self.group_of(player) {
                Some(i) => {
                    let g = &mut self.groups[i];
                    g.isolated = !g.isolated;
                    self.dirty = true;
                    if g.isolated {
                        vec![format!("{} is isolated: only its members hear each other.", g.name)]
                    } else {
                        vec![format!("{} is open: players nearby hear its members again.", g.name)]
                    }
                }
                None => vec!["You are not in a voice group.".into()],
            },
            "invite" => match self.group_of(player) {
                Some(i) => {
                    let target = words.next().unwrap_or("");
                    let g = &self.groups[i];
                    match self.players.iter().find(|p| p.name.eq_ignore_ascii_case(target)) {
                        Some(p) => {
                            let join = match &g.password {
                                Some(pw) => format!("/voicechat join {} {pw}", g.name),
                                None => format!("/voicechat join {}", g.name),
                            };
                            others.push((
                                p.uuid,
                                format!("{} invited you to the voice group {}: {join}", name(self, player), g.name),
                            ));
                            vec![format!("You invited {} to {}.", p.name, g.name)]
                        }
                        None => vec![format!("{target} is not online.")],
                    }
                }
                None => vec!["Join a voice group first: /voicechat join <group>.".into()],
            },
            _ => vec![
                "Simple Voice Chat:".into(),
                "/voicechat join <group> [password] - join or make a group".into(),
                "/voicechat leave - leave your group".into(),
                "/voicechat groups - list the groups".into(),
                "/voicechat isolate - only your group hears your group".into(),
                "/voicechat invite <player> - invite a player to your group".into(),
            ],
        };
        (reply, others)
    }

    fn leave(&mut self, player: u128, others: &mut Vec<(u128, String)>) -> bool {
        let Some(i) = self.group_of(player) else {
            return false;
        };
        self.groups[i].members.retain(|&m| m != player);
        let who = self
            .players
            .iter()
            .find(|p| p.uuid == player)
            .map_or("someone", |p| p.name.as_str())
            .to_owned();
        for &m in &self.groups[i].members {
            others.push((m, format!("{who} left the voice group.")));
        }
        self.groups.retain(|g| !g.members.is_empty());
        self.dirty = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(uuid: u128, name: &str, dim: &str, x: f64) -> Player {
        Player {
            uuid,
            name: name.into(),
            dimension: dim.into(),
            position: [x, 64.0, 0.0],
        }
    }

    fn world() -> VoiceServer {
        let mut s = VoiceServer::default();
        s.tick(vec![
            player(1, "Alex", "minecraft:overworld", 0.0),
            player(2, "Steve", "minecraft:overworld", 30.0),
            player(3, "Far", "minecraft:overworld", 100.0),
            player(4, "Nether", "minecraft:the_nether", 0.0),
        ]);
        for u in 1..=4 {
            s.message(u, proto::HELLO, &Hello { disabled: false }.encode());
        }
        s.tick(s.players.clone());
        s.drain();
        s
    }

    fn talk(s: &mut VoiceServer, from: u128, whisper: bool) -> Vec<(u128, Sound)> {
        let mic = Mic {
            whisper,
            seq: 1,
            frames: vec![vec![0; 243]],
        };
        s.message(from, proto::MIC, &mic.encode());
        s.drain()
            .into_iter()
            .filter(|m| m.1 == proto::SOUND)
            .map(|(u, _, d)| (u, Sound::decode(&d).unwrap()))
            .collect()
    }

    fn heard(s: &mut VoiceServer, from: u128, whisper: bool) -> Vec<u128> {
        talk(s, from, whisper).into_iter().map(|m| m.0).collect()
    }

    #[test]
    fn nearby_players_in_the_same_dimension_hear() {
        let mut s = world();
        assert_eq!(heard(&mut s, 1, false), [2]);
        // A whisper carries 24 blocks: Steve is 30 away.
        assert!(heard(&mut s, 1, true).is_empty());
        let sound = &talk(&mut s, 1, false)[0].1;
        assert_eq!(
            sound.kind,
            SoundKind::Proximity {
                pos: [0.0, 65.62, 0.0],
                range: 48.0
            }
        );
    }

    #[test]
    fn players_without_the_mod_or_with_it_off_hear_nothing() {
        let mut s = world();
        s.message(2, proto::HELLO, &Hello { disabled: true }.encode());
        assert!(heard(&mut s, 1, false).is_empty());
        assert!(heard(&mut s, 2, false).is_empty(), "nor are they heard");
        // An unknown player's frames go nowhere.
        assert!(heard(&mut s, 9, false).is_empty());
    }

    #[test]
    fn groups_hear_each_other_anywhere() {
        let mut s = world();
        s.command(1, "voicechat join team");
        let (reply, others) = s.command(4, "voicechat join team");
        assert_eq!(reply, ["You joined the voice group team."]);
        assert_eq!(others, [(1, "Nether joined the voice group.".to_owned())]);
        let mut h = heard(&mut s, 1, false);
        h.sort();
        assert_eq!(h, [2, 4], "Steve nearby, Nether through the group");
        assert_eq!(talk(&mut s, 1, false).iter().find(|m| m.0 == 4).unwrap().1.kind, SoundKind::Group);
        s.command(1, "voicechat isolate");
        assert_eq!(heard(&mut s, 1, false), [4]);
        assert!(heard(&mut s, 2, false).is_empty(), "isolated members don't hear outsiders");
        s.command(1, "voicechat leave");
        assert_eq!(heard(&mut s, 1, false), [2]);
    }

    #[test]
    fn passwords_and_leaving() {
        let mut s = world();
        s.command(1, "voicechat join secret hunter2");
        assert_eq!(s.command(2, "voicechat join secret").0, ["Wrong password for secret."]);
        assert_eq!(s.command(2, "voicechat join secret hunter2").0, ["You joined the voice group secret."]);
        // Leaving the world leaves the group, and an empty group goes.
        s.tick(vec![player(3, "Far", "minecraft:overworld", 100.0)]);
        assert_eq!(s.command(3, "voicechat groups").0.len(), 1);
        assert!(s.groups.is_empty());
    }

    #[test]
    fn everyone_with_the_mod_hears_who_is_there() {
        let mut s = world();
        s.command(1, "voicechat join team");
        s.command(2, "voicechat join team");
        s.tick(s.players.clone());
        let states: HashMap<u128, State> = s
            .drain()
            .into_iter()
            .filter(|m| m.1 == proto::STATE)
            .map(|(u, _, d)| (u, State::decode(&d).unwrap()))
            .collect();
        assert_eq!(states.len(), 4);
        let alex = &states[&1];
        assert_eq!(alex.group.as_deref(), Some("team"));
        assert_eq!(alex.peers.len(), 3);
        assert!(alex.peers.iter().any(|p| p.name == "Steve" && p.grouped));
        assert!(alex.peers.iter().any(|p| p.name == "Far" && !p.grouped));
    }
}
