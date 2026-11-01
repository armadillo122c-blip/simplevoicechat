// Push to talk, whisper, mute and voice chat off: the player's choice of keys.
fn svc_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use simplevoicechat_engine::{self as svc, client::Key};
    if !game.native_on(svc::ID) {
        return;
    }
    let playing = game.native_playing();
    for k in Key::ALL {
        if game.native_mod_key(svc::ID, k.name(), k.default_key()) == key {
            svc::with_client(|c| c.key(k, pressed, playing));
        }
    }
}
