//! The messages between the halves, each on its own `simplevoicechat:` channel. Little endian;
//! a string is a length byte and UTF-8. Anything that doesn't read whole is dropped.

/// Client to server: the client has the mod, and whether its player turned voice chat off.
pub const HELLO: &str = "simplevoicechat:hello";
/// Client to server: the frames recorded this tick.
pub const MIC: &str = "simplevoicechat:mic";
/// Server to client: frames of a voice this player hears.
pub const SOUND: &str = "simplevoicechat:sound";
/// Server to client: the players with voice chat, this player's group and the groups.
pub const STATE: &str = "simplevoicechat:state";
/// Client to server: a group action from the menu (`join <group> [password]`, `leave`,
/// `isolate`), answered in chat as the command is.
pub const CMD: &str = "simplevoicechat:cmd";

/// Bumped when a message changes shape; halves of other versions ignore each other.
pub const PROTOCOL: u8 = 2;
/// Frames a message carries at most (a tick holds 2 or 3).
pub const MAX_FRAMES: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct Hello {
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mic {
    pub whisper: bool,
    pub seq: u32,
    pub frames: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundKind {
    /// A nearby voice, at the speaker's eyes, fading out at `range`.
    Proximity { pos: [f64; 3], range: f32 },
    /// A group member's voice: the same anywhere.
    Group,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub sender: u128,
    pub kind: SoundKind,
    pub whisper: bool,
    pub seq: u32,
    pub frames: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub uuid: u128,
    pub name: String,
    pub disabled: bool,
    /// In the listener's group.
    pub grouped: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub group: Option<String>,
    pub isolated: bool,
    /// The server's voice distance, in blocks.
    pub distance: f32,
    pub peers: Vec<Peer>,
    /// Every group on the server.
    pub groups: Vec<GroupInfo>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupInfo {
    pub name: String,
    pub members: u8,
    pub password: bool,
    pub isolated: bool,
}

/// A group action from the menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Cmd {
    pub line: String,
}

impl Cmd {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(PROTOCOL).str(&self.line);
        w.0
    }
    pub fn decode(data: &[u8]) -> Option<Self> {
        let mut r = header(data)?;
        let line = r.str()?;
        r.end(Self { line })
    }
}

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
    fn str(&mut self, v: &str) -> &mut Self {
        let mut end = v.len().min(255);
        while !v.is_char_boundary(end) {
            end -= 1;
        }
        self.u8(end as u8).bytes(&v.as_bytes()[..end])
    }
    fn frames(&mut self, frames: &[Vec<u8>]) -> &mut Self {
        let frames = &frames[..frames.len().min(MAX_FRAMES)];
        self.u8(frames.len() as u8);
        for f in frames {
            self.bytes(&(f.len().min(u16::MAX as usize) as u16).to_le_bytes())
                .bytes(&f[..f.len().min(u16::MAX as usize)]);
        }
        self
    }
}

struct R<'a>(&'a [u8]);

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn arr<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u8()?;
        String::from_utf8(self.take(n.into())?.to_vec()).ok()
    }
    fn frames(&mut self) -> Option<Vec<Vec<u8>>> {
        let n = usize::from(self.u8()?);
        if n > MAX_FRAMES {
            return None;
        }
        (0..n)
            .map(|_| {
                let len = u16::from_le_bytes(self.arr()?);
                Some(self.take(len.into())?.to_vec())
            })
            .collect()
    }
    /// The rest must be empty: a message is read whole or not at all.
    fn end<T>(self, v: T) -> Option<T> {
        self.0.is_empty().then_some(v)
    }
}

/// Reads the protocol byte that starts every message.
fn header<'a>(data: &'a [u8]) -> Option<R<'a>> {
    let mut r = R(data);
    (r.u8()? == PROTOCOL).then_some(r)
}

impl Hello {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(PROTOCOL).u8(u8::from(self.disabled));
        w.0
    }
    pub fn decode(data: &[u8]) -> Option<Self> {
        let mut r = header(data)?;
        let disabled = r.u8()? != 0;
        r.end(Self { disabled })
    }
}

impl Mic {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(PROTOCOL)
            .u8(u8::from(self.whisper))
            .bytes(&self.seq.to_le_bytes())
            .frames(&self.frames);
        w.0
    }
    pub fn decode(data: &[u8]) -> Option<Self> {
        let mut r = header(data)?;
        let whisper = r.u8()? != 0;
        let seq = u32::from_le_bytes(r.arr()?);
        let frames = r.frames()?;
        r.end(Self {
            whisper,
            seq,
            frames,
        })
    }
}

impl Sound {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(PROTOCOL).bytes(&self.sender.to_le_bytes());
        match self.kind {
            SoundKind::Proximity { pos, range } => {
                w.u8(0);
                for c in pos {
                    w.bytes(&c.to_le_bytes());
                }
                w.bytes(&range.to_le_bytes());
            }
            SoundKind::Group => {
                w.u8(1);
            }
        }
        w.u8(u8::from(self.whisper))
            .bytes(&self.seq.to_le_bytes())
            .frames(&self.frames);
        w.0
    }
    pub fn decode(data: &[u8]) -> Option<Self> {
        let mut r = header(data)?;
        let sender = u128::from_le_bytes(r.arr()?);
        let kind = match r.u8()? {
            0 => {
                let mut pos = [0.0; 3];
                for c in &mut pos {
                    *c = f64::from_le_bytes(r.arr()?);
                }
                let range = f32::from_le_bytes(r.arr()?);
                if !pos.iter().all(|c| c.is_finite()) || !range.is_finite() {
                    return None;
                }
                SoundKind::Proximity { pos, range }
            }
            1 => SoundKind::Group,
            _ => return None,
        };
        let whisper = r.u8()? != 0;
        let seq = u32::from_le_bytes(r.arr()?);
        let frames = r.frames()?;
        r.end(Self {
            sender,
            kind,
            whisper,
            seq,
            frames,
        })
    }
}

impl State {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(PROTOCOL);
        match &self.group {
            Some(g) => w.u8(1).str(g),
            None => w.u8(0),
        };
        w.u8(u8::from(self.isolated))
            .bytes(&self.distance.to_le_bytes())
            .bytes(&(self.peers.len().min(u16::MAX as usize) as u16).to_le_bytes());
        for p in self.peers.iter().take(u16::MAX as usize) {
            w.bytes(&p.uuid.to_le_bytes())
                .str(&p.name)
                .u8(u8::from(p.disabled) | u8::from(p.grouped) << 1);
        }
        w.u8(self.groups.len().min(255) as u8);
        for g in self.groups.iter().take(255) {
            w.str(&g.name)
                .u8(g.members)
                .u8(u8::from(g.password) | u8::from(g.isolated) << 1);
        }
        w.0
    }
    pub fn decode(data: &[u8]) -> Option<Self> {
        let mut r = header(data)?;
        let group = match r.u8()? {
            0 => None,
            _ => Some(r.str()?),
        };
        let isolated = r.u8()? != 0;
        let distance = f32::from_le_bytes(r.arr()?);
        let n = u16::from_le_bytes(r.arr()?);
        let peers = (0..n)
            .map(|_| {
                let uuid = u128::from_le_bytes(r.arr()?);
                let name = r.str()?;
                let flags = r.u8()?;
                Some(Peer {
                    uuid,
                    name,
                    disabled: flags & 1 != 0,
                    grouped: flags & 2 != 0,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let n = r.u8()?;
        let groups = (0..n)
            .map(|_| {
                let name = r.str()?;
                let members = r.u8()?;
                let flags = r.u8()?;
                Some(GroupInfo {
                    name,
                    members,
                    password: flags & 1 != 0,
                    isolated: flags & 2 != 0,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        r.end(Self {
            group,
            isolated,
            distance,
            peers,
            groups,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let hello = Hello { disabled: true };
        assert_eq!(Hello::decode(&hello.encode()), Some(hello));
        let mic = Mic {
            whisper: true,
            seq: 7,
            frames: vec![vec![1, 2, 3], vec![9; 243]],
        };
        assert_eq!(Mic::decode(&mic.encode()), Some(mic.clone()));
        for kind in [
            SoundKind::Group,
            SoundKind::Proximity {
                pos: [1.5, -64.0, 1e6],
                range: 48.0,
            },
        ] {
            let sound = Sound {
                sender: u128::MAX - 5,
                kind,
                whisper: false,
                seq: 99,
                frames: mic.frames.clone(),
            };
            assert_eq!(Sound::decode(&sound.encode()), Some(sound));
        }
        let state = State {
            group: Some("miners".into()),
            isolated: true,
            distance: 48.0,
            peers: vec![
                Peer {
                    uuid: 1,
                    name: "Armadillo122".into(),
                    disabled: false,
                    grouped: true,
                },
                Peer {
                    uuid: 2,
                    name: "Steve".into(),
                    disabled: true,
                    grouped: false,
                },
            ],
            groups: vec![GroupInfo {
                name: "miners".into(),
                members: 2,
                password: true,
                isolated: true,
            }],
        };
        assert_eq!(State::decode(&state.encode()), Some(state));
        let cmd = Cmd {
            line: "join miners hunter2".into(),
        };
        assert_eq!(Cmd::decode(&cmd.encode()), Some(cmd));
    }

    #[test]
    fn short_long_and_foreign_messages_are_dropped() {
        let mic = Mic {
            whisper: false,
            seq: 1,
            frames: vec![vec![0; 243]],
        }
        .encode();
        assert!(Mic::decode(&mic[..mic.len() - 1]).is_none());
        let mut long = mic.clone();
        long.push(0);
        assert!(Mic::decode(&long).is_none());
        let mut other = mic.clone();
        other[0] = PROTOCOL + 1;
        assert!(Mic::decode(&other).is_none());
        assert!(Hello::decode(&[]).is_none());
    }
}
