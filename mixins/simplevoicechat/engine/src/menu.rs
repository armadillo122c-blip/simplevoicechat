//! The voice chat menu (V), as Simple Voice Chat's: the microphone and voice toggles, every
//! player with voice chat and their volume, and the groups. It is laid out here as shapes and
//! click areas; the helpers draw the shapes with the game's sprites and text, and hand clicks
//! back by position.
use crate::proto::GroupInfo;

/// What the menu shows.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub connected: bool,
    pub muted: bool,
    pub disabled: bool,
    pub hud: bool,
    pub voice_activation: bool,
    pub mic_test: bool,
    pub peers: Vec<PeerRow>,
    /// The group you are in, and whether it is isolated.
    pub group: Option<(String, bool)>,
    pub groups: Vec<GroupInfo>,
    /// The text box: a new group's name, or a password to join with.
    pub field: String,
    pub field_focus: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PeerRow {
    pub uuid: u128,
    pub name: String,
    pub volume: u32,
    pub speaking: bool,
    pub grouped: bool,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Mute,
    Disable,
    Hud,
    Activation,
    MicTest,
    /// A player's volume, up or down a step.
    Volume(u128, i32),
    Join(String),
    Create,
    Leave,
    Isolate,
    Field,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// `[x, y, w, h]`, ARGB.
    Fill([f32; 4], u32),
    /// Text at its top left, RGB, with a shadow.
    Text(String, [f32; 2], u32),
    /// Part `src` of sprite `name` drawn into `dst`.
    Sprite(&'static str, [f32; 4], [f32; 4]),
}

#[derive(Default)]
pub struct Layout {
    pub shapes: Vec<Shape>,
    pub hits: Vec<([f32; 4], Action)>,
}

impl Layout {
    /// The action under `at`, if any.
    pub fn hit(&self, at: [f32; 2]) -> Option<Action> {
        self.hits
            .iter()
            .find(|(r, _)| inside(*r, at))
            .map(|(_, a)| a.clone())
    }
}

fn inside(r: [f32; 4], at: [f32; 2]) -> bool {
    at[0] >= r[0] && at[0] < r[0] + r[2] && at[1] >= r[1] && at[1] < r[1] + r[3]
}

const W: f32 = 280.0;
const PAD: f32 = 8.0;
const ROW: f32 = 20.0;
const GAP: f32 = 4.0;
const LINE: f32 = 12.0;
/// Rows of players and of groups shown at most.
const MAX_PEERS: usize = 6;
const MAX_GROUPS: usize = 4;

const WHITE: u32 = 0xFFFFFF;
const GRAY: u32 = 0xA0A0A0;
const DARK: u32 = 0x707070;
const GREEN: u32 = 0x55FF55;
const RED: u32 = 0xFF5555;
const YELLOW: u32 = 0xFFFF55;

/// Builds the shapes in order, with the mouse for hover highlights.
struct Builder<'a> {
    out: Layout,
    mouse: [f32; 2],
    width: &'a dyn Fn(&str) -> f32,
}

impl Builder<'_> {
    fn text(&mut self, s: &str, at: [f32; 2], rgb: u32) {
        self.out.shapes.push(Shape::Text(s.to_owned(), at, rgb));
    }

    /// A text cut to `max` pixels, with an ellipsis.
    fn fit(&self, s: &str, max: f32) -> String {
        if (self.width)(s) <= max {
            return s.to_owned();
        }
        let mut t: String = s.to_owned();
        while !t.is_empty() && (self.width)(&format!("{t}...")) > max {
            t.pop();
        }
        format!("{t}...")
    }

    /// A vanilla button: its sprite cut in two halves so any width keeps both edges.
    fn button(&mut self, r: [f32; 4], label: &str, action: Option<Action>) {
        let hovered = action.is_some() && inside(r, self.mouse);
        let sprite = match (action.is_some(), hovered) {
            (false, _) => "gui/sprites/widget/button_disabled",
            (true, true) => "gui/sprites/widget/button_highlighted",
            (true, false) => "gui/sprites/widget/button",
        };
        self.sliced(sprite, r);
        let label = self.fit(label, r[2] - 6.0);
        let w = (self.width)(&label);
        let colour = if action.is_some() { WHITE } else { GRAY };
        self.text(&label, [r[0] + (r[2] - w) / 2.0, r[1] + (r[3] - 8.0) / 2.0], colour);
        if let Some(a) = action {
            self.out.hits.push((r, a));
        }
    }

    fn sliced(&mut self, sprite: &'static str, r: [f32; 4]) {
        // Button and text box sprites are 200 x 20.
        let half = (r[2] / 2.0).min(100.0);
        self.out
            .shapes
            .push(Shape::Sprite(sprite, [0.0, 0.0, half, 20.0], [r[0], r[1], half, r[3]]));
        self.out.shapes.push(Shape::Sprite(
            sprite,
            [200.0 - half, 0.0, half, 20.0],
            [r[0] + half, r[1], r[2] - half, r[3]],
        ));
    }
}

/// The menu centred in `screen` (`[x, y, w, h]`, GUI pixels), with the mouse at `mouse`.
pub fn layout(v: &View, screen: [f32; 4], mouse: [f32; 2], width: &dyn Fn(&str) -> f32) -> Layout {
    // As many list rows as the screen has room for: the group's first, then the players'.
    let fixed = PAD * 2.0 + LINE * 2.0 + (ROW + GAP) * 2.0 + LINE + GAP + LINE;
    let slots = (((screen[3] - 8.0 - fixed) / (ROW + 2.0)).floor() as usize).max(3);
    let max_groups = match &v.group {
        Some(_) => 0,
        None => v.groups.len().min(MAX_GROUPS).min(slots.saturating_sub(2) / 2),
    };
    let group_rows = if v.group.is_some() { 1 } else { max_groups + 1 };
    let peer_slots = slots.saturating_sub(group_rows).clamp(1, MAX_PEERS);
    let shown_peers = if v.peers.len() > peer_slots {
        peer_slots - 1
    } else {
        v.peers.len()
    };
    let peer_rows = if v.peers.len() > peer_slots {
        peer_slots
    } else {
        v.peers.len().max(1)
    };
    let h = fixed + (peer_rows + group_rows) as f32 * (ROW + 2.0);
    let x = (screen[0] + (screen[2] - W) / 2.0).round();
    let y = (screen[1] + (screen[3] - h) / 2.0).max(screen[1] + 4.0).round();
    let mut b = Builder {
        out: Layout::default(),
        mouse,
        width,
    };
    b.out.shapes.push(Shape::Fill([x - 1.0, y - 1.0, W + 2.0, h + 2.0], 0xFF00_0000));
    b.out.shapes.push(Shape::Fill([x, y, W, h], 0xE020_2020));
    let inner = W - PAD * 2.0;
    let (left, mut cy) = (x + PAD, y + PAD);

    let title = "Voice Chat";
    b.text(title, [x + (W - width(title)) / 2.0, cy], WHITE);
    if !v.connected {
        let s = "This world has no voice chat";
        b.text(s, [x + (W - width(s)) / 2.0, cy + LINE], RED);
    } else if v.disabled {
        let s = "Voice chat is off";
        b.text(s, [x + (W - width(s)) / 2.0, cy + LINE], RED);
    }
    cy += LINE * 2.0;

    // The toggles: three, then two.
    let on = |b: bool| if b { "ON" } else { "OFF" };
    let third = (inner - GAP * 2.0) / 3.0;
    let mic = if v.muted { "Mic: Muted" } else { "Mic: On" };
    b.button([left, cy, third, ROW], mic, Some(Action::Mute));
    let voice = format!("Voice: {}", on(!v.disabled));
    b.button([left + third + GAP, cy, third, ROW], &voice, Some(Action::Disable));
    let hud = format!("HUD: {}", if v.hud { "Shown" } else { "Hidden" });
    b.button([left + (third + GAP) * 2.0, cy, third, ROW], &hud, Some(Action::Hud));
    cy += ROW + GAP;
    let half = (inner - GAP) / 2.0;
    let act = if v.voice_activation { "Activation: Voice" } else { "Activation: Push to talk" };
    b.button([left, cy, half, ROW], act, Some(Action::Activation));
    let test = format!("Hear yourself: {}", on(v.mic_test));
    b.button([left + half + GAP, cy, half, ROW], &test, Some(Action::MicTest));
    cy += ROW + GAP;

    // The players, with their volume.
    b.text("Players", [left, cy], YELLOW);
    cy += LINE;
    if v.peers.is_empty() {
        b.text("Nobody else has voice chat here", [left + 4.0, cy + 6.0], GRAY);
        cy += ROW + 2.0;
    }
    let small = 20.0;
    let pct_w = 36.0;
    for p in v.peers.iter().take(shown_peers) {
        let colour = if p.disabled {
            DARK
        } else if p.speaking {
            GREEN
        } else {
            WHITE
        };
        let mut name = p.name.clone();
        if p.grouped {
            name.push_str(" [group]");
        }
        if p.disabled {
            name.push_str(" (off)");
        }
        let controls = small * 2.0 + pct_w + GAP * 2.0;
        let name = b.fit(&name, inner - controls - 8.0);
        b.out.shapes.push(Shape::Fill([left, cy, inner, ROW], 0x4000_0000));
        b.text(&name, [left + 4.0, cy + 6.0], colour);
        let bx = left + inner - controls;
        b.button([bx, cy, small, ROW], "-", (p.volume > 0).then_some(Action::Volume(p.uuid, -1)));
        let pct = format!("{}%", p.volume);
        b.text(&pct, [bx + small + GAP + (pct_w - width(&pct)) / 2.0, cy + 6.0], WHITE);
        let up = (p.volume < 200).then_some(Action::Volume(p.uuid, 1));
        b.button([bx + small + GAP * 2.0 + pct_w, cy, small, ROW], "+", up);
        cy += ROW + 2.0;
    }
    if v.peers.len() > shown_peers {
        b.text(&format!("and {} more", v.peers.len() - shown_peers), [left + 4.0, cy + 6.0], GRAY);
        cy += ROW + 2.0;
    }
    cy += GAP;

    // The groups.
    b.text("Group", [left, cy], YELLOW);
    cy += LINE;
    let btn = 60.0;
    match &v.group {
        Some((name, isolated)) => {
            let label = format!("{name}{}", if *isolated { " (isolated)" } else { "" });
            let label = b.fit(&label, inner - btn * 2.0 - GAP * 3.0);
            b.text(&label, [left + 4.0, cy + 6.0], GREEN);
            let iso = if *isolated { "Open" } else { "Isolate" };
            b.button([left + inner - btn * 2.0 - GAP, cy, btn, ROW], iso, Some(Action::Isolate));
            b.button([left + inner - btn, cy, btn, ROW], "Leave", Some(Action::Leave));
        }
        None => {
            for g in v.groups.iter().take(max_groups) {
                let mut label = format!("{} ({})", g.name, g.members);
                if g.password {
                    label.push_str(" - password");
                }
                if g.isolated {
                    label.push_str(" - isolated");
                }
                let label = b.fit(&label, inner - btn - GAP - 8.0);
                b.out.shapes.push(Shape::Fill([left, cy, inner, ROW], 0x4000_0000));
                b.text(&label, [left + 4.0, cy + 6.0], WHITE);
                let join = v.connected.then(|| Action::Join(g.name.clone()));
                b.button([left + inner - btn, cy, btn, ROW], "Join", join);
                cy += ROW + 2.0;
            }
            let field = [left, cy, inner - btn - GAP, ROW];
            let sprite = if v.field_focus {
                "gui/sprites/widget/text_field_highlighted"
            } else {
                "gui/sprites/widget/text_field"
            };
            b.sliced(sprite, field);
            if v.field.is_empty() && !v.field_focus {
                b.text("Group name, or a password to join", [field[0] + 4.0, cy + 6.0], DARK);
            } else {
                let caret = if v.field_focus { "_" } else { "" };
                let shown = format!("{}{caret}", v.field);
                // Keep the end in view while typing.
                let mut start = 0;
                while width(&shown[start..]) > field[2] - 8.0 && start < shown.len() {
                    start += shown[start..].chars().next().map_or(1, char::len_utf8);
                }
                b.text(&shown[start..], [field[0] + 4.0, cy + 6.0], WHITE);
            }
            b.out.hits.push((field, Action::Field));
            let create = (v.connected && !v.field.trim().is_empty()).then_some(Action::Create);
            b.button([left + inner - btn, cy, btn, ROW], "Create", create);
        }
    }
    b.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(s: &str) -> f32 {
        s.chars().count() as f32 * 6.0
    }

    fn view() -> View {
        View {
            connected: true,
            hud: true,
            peers: vec![PeerRow {
                uuid: 7,
                name: "AngryChud".into(),
                volume: 100,
                speaking: true,
                grouped: false,
                disabled: false,
            }],
            groups: vec![GroupInfo {
                name: "team".into(),
                members: 2,
                password: false,
                isolated: false,
            }],
            ..Default::default()
        }
    }

    fn centre(l: &Layout, a: &Action) -> [f32; 2] {
        let (r, _) = l.hits.iter().find(|(_, x)| x == a).expect("the action has a button");
        [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
    }

    #[test]
    fn every_button_is_where_a_click_finds_it() {
        let screen = [0.0, 0.0, 427.0, 240.0];
        let l = layout(&view(), screen, [0.0, 0.0], &width);
        for a in [
            Action::Mute,
            Action::Disable,
            Action::Hud,
            Action::Activation,
            Action::MicTest,
            Action::Volume(7, -1),
            Action::Volume(7, 1),
            Action::Join("team".into()),
            Action::Field,
        ] {
            assert_eq!(l.hit(centre(&l, &a)), Some(a));
        }
        assert_eq!(l.hit([1.0, 1.0]), None, "outside the panel");
        // Create waits for a name.
        assert!(!l.hits.iter().any(|(_, a)| *a == Action::Create));
        let typed = View {
            field: "miners".into(),
            ..view()
        };
        let l = layout(&typed, screen, [0.0, 0.0], &width);
        assert_eq!(l.hit(centre(&l, &Action::Create)), Some(Action::Create));
    }

    #[test]
    fn in_a_group_it_offers_leave_and_isolate_and_fits_the_screen() {
        let v = View {
            group: Some(("team".into(), false)),
            ..view()
        };
        let screen = [0.0, 0.0, 427.0, 240.0];
        let l = layout(&v, screen, [0.0, 0.0], &width);
        assert!(l.hits.iter().any(|(_, a)| *a == Action::Leave));
        assert!(l.hits.iter().any(|(_, a)| *a == Action::Isolate));
        assert!(!l.hits.iter().any(|(_, a)| matches!(a, Action::Join(_))));
        for (r, _) in &l.hits {
            assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[0] + r[2] <= 427.0 && r[1] + r[3] <= 240.0, "{r:?}");
        }
    }

    #[test]
    fn hovering_lights_a_button() {
        let l = layout(&view(), [0.0, 0.0, 427.0, 240.0], [0.0, 0.0], &width);
        let at = centre(&l, &Action::Mute);
        let lit = layout(&view(), [0.0, 0.0, 427.0, 240.0], at, &width);
        assert!(lit.shapes.iter().any(|s| matches!(s, Shape::Sprite("gui/sprites/widget/button_highlighted", ..))));
    }
}
