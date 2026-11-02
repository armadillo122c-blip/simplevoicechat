// The mod's keys: the menu (V) and groups (G), push to talk, whisper, mute, voice chat off and
// the HUD, each the player's choice of key.
fn svc_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use simplevoicechat_engine::{
        self as svc,
        client::{Key, KeyOutcome},
    };
    if !game.native_on(svc::ID) {
        return;
    }
    let playing = game.native_playing();
    for k in Key::ALL {
        if game.native_mod_key(svc::ID, k.name(), k.default_key()) != key {
            continue;
        }
        match svc::with_client(|c| c.key(k, pressed, playing)) {
            KeyOutcome::OpenMenu => {
                game.native_mod_screen_open(svc::ID);
            }
            KeyOutcome::Store => {
                let stored = {
                    let game = &*game;
                    svc::with_client(|c| c.settings(|key| game.native_setting(svc::ID, key)).stored())
                };
                for (key, value) in stored {
                    game.native_set_setting(svc::ID, key, &value);
                }
            }
            KeyOutcome::None => {}
        }
    }
}