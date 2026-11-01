# Simple Voice Chat for Horizon

Proximity voice chat for [Horizon](https://playhorizon.dev/), the Rust edition of Minecraft: talk to
the players near you, or to your group anywhere in the world. A Horizon port of henkelmax's
[Simple Voice Chat](https://github.com/henkelmax/simple-voice-chat).

It is a **native mod**: a [horizon-mixins](https://github.com/horizon-rust/horizon-mixins) bundle
(`mixins/simplevoicechat/`) that the Horizon launcher compiles into the game. Java mods can't run in
Horizon, so this is a rewrite in Rust, not a copy of the Java code.

## What it does

- **Proximity voice.** Players within 48 blocks hear you (the host can change it), quieter with
  distance and panned left or right by where you stand.
- **Push to talk or voice activation**, with a threshold, plus microphone amplification.
- **Whisper**: hold the whisper key and your voice carries half as far.
- **Groups**: `/voicechat join <group> [password]` makes or joins one. Members hear each other
  anywhere, in any dimension. `/voicechat isolate` makes the group private.
- **Mute** and **turn voice chat off** keys.
- **HUD box** (top left): your mic state, your group and who is talking.
- **Mic test**: hear yourself through the codec.
- **Device picker**: choose the microphone and speakers on the settings page.

### Keys (Options > Mods > Simple Voice Chat)

| Key | Default |
|---|---|
| Push to talk | Caps Lock |
| Whisper (hold) | B |
| Mute microphone | M |
| Turn voice chat off | N |

### Commands

| Command | What it does |
|---|---|
| `/voicechat join <group> [password]` | Join a group, or make it if it doesn't exist |
| `/voicechat leave` | Leave your group |
| `/voicechat groups` | List the groups and their members |
| `/voicechat isolate` | Toggle your group between open and isolated |
| `/voicechat invite <player>` | Send a player the command to join your group |

## How it compares to the Java mod

| | Java Simple Voice Chat | This port |
|---|---|---|
| Transport | Its own UDP port (24454), AES-encrypted | Horizon's mod messages over the game connection: no port to open |
| Codec | Opus | IMA ADPCM at 24 kHz, about 12 KB/s per speaker. Horizon only lets native mods use crates in its `Cargo.lock`, and Opus isn't one |
| 3D sound | OpenAL | Distance fade and stereo panning |
| Noise suppression (RNNoise) | Yes | Not yet |
| Speaking icons over heads | Yes | HUD list instead |
| Talking to Java servers or clients | n/a | No. Horizon players only |

### Where it works

Native mods run in your own game and its integrated server. **The host and every player who joins
need this mod.**

- **Single player opened to LAN, or a Horizon-hosted world:** works for everyone who has the mod.
- **Dedicated Horizon servers:** not yet. Horizon doesn't build native mods for dedicated servers.
- **Vanilla or Java servers:** no voice. The client shows "Server has no voice chat".

## Install

The launcher compiles native mods into the game, so building one runs code on your computer.

1. In the Horizon launcher: **Mods → Add from GitHub**, paste
   `https://github.com/armadillo122c-blip/simplevoicechat`, then **Build**. From a terminal:
   `horizon-launcher mods add-github https://github.com/armadillo122c-blip/simplevoicechat --trust-native-code`.
2. The first native build downloads Rust and takes 20 to 30 minutes. Later builds are faster.
3. In game: **Options → Mods → Simple Voice Chat → Settings** to choose push to talk or voice
   activation and your devices.

## Layout

| Path | What it is |
|---|---|
| `mixins/simplevoicechat/horizon-mixin.toml` | The bundle: its crate and one HEAD hook per hook point |
| `mixins/simplevoicechat/*.rs` | One small helper per hook (glue only) |
| `mixins/simplevoicechat/engine/` | The engine crate, which uses `cpal` from Horizon's lock |
| `engine/src/client.rs` | Settings, keys, voice activation, sending frames, HUD lines |
| `engine/src/server.rs` | Players, groups, routing, `/voicechat` |
| `engine/src/codec.rs` | The ADPCM voice codec |
| `engine/src/audio.rs` | Microphone and speakers (cpal), jitter buffers, the mixer |
| `engine/src/spatial.rs` | Distance fade and panning |
| `engine/src/proto.rs` | The four messages on `simplevoicechat:*` channels |

Messages: `hello` (client has the mod, on or off), `mic` (a tick's frames), `sound` (frames
forwarded with the speaker's position or as group voice) and `state` (players with voice, your
group).

## Develop

```sh
cargo test        # the engine: codec, routing, groups, settings, mixer, panning
```

To check the bundle against Horizon, apply it to the pinned commit with horizon-mixins and run
`cargo check -p horizon-client` on the staged source, as
[horizon-native-mod-template](https://github.com/horizon-rust/horizon-native-mod-template)'s
`tools/horizon.py` does. It pins Nightly 2026-11-01 (`c9d474d`) and uses only the native hook
points (`"hooks": 57`).

## Credits

The idea, features and defaults come from [Simple Voice Chat](https://github.com/henkelmax/simple-voice-chat)
by Max Henkel. This Rust port shares no code with it and isn't affiliated with it. Horizon is by
[horizon-rust](https://github.com/horizon-rust/horizon).

License: MIT.
