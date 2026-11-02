// A click on the voice chat menu: its buttons, the volumes, the groups and the text box.
fn svc_menu_click(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    use simplevoicechat_engine::{self as svc, menu};
    if game.native_mod_screen() != Some(svc::ID) || button != 0 {
        return;
    }
    let view = svc::with_client(|c| c.menu_view());
    let layout = menu::layout(&view, screen.rect, screen.mouse, &|t| game.native_text_width(t));
    *canceled = true;
    let Some(action) = layout.hit(screen.mouse) else {
        svc::with_client(|c| c.menu_blur());
        return;
    };
    let stored = svc::with_client(|c| {
        c.menu_action(action)
            .then(|| c.settings(|key| game.native_setting(svc::ID, key)).stored())
    });
    for (key, value) in stored.unwrap_or_default() {
        game.native_set_setting(svc::ID, key, &value);
    }
}