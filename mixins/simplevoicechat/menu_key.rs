// A key on the voice chat menu: typing in its text box, or the menu key closing it.
fn svc_menu_key(
    game: &crate::game::Game<'_>,
    key: &str,
    canceled: &mut bool,
) {
    use simplevoicechat_engine::{
        self as svc,
        client::{Key, MenuKey},
    };
    if game.native_mod_screen() != Some(svc::ID) {
        return;
    }
    let menu_keys: Vec<String> = [Key::Menu, Key::Group]
        .iter()
        .map(|k| game.native_mod_key(svc::ID, k.name(), k.default_key()))
        .collect();
    if svc::with_client(|c| c.menu_key(key, &menu_keys)) != MenuKey::Pass {
        *canceled = true;
    }
}