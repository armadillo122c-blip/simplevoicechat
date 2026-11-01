// A click on Simple Voice Chat's settings page: change the setting and keep it.
fn svc_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use simplevoicechat_engine as svc;
    if mod_id != svc::ID {
        return;
    }
    let stored = {
        let game = &*game;
        svc::with_client(|c| {
            let mut settings = c.settings(|key| game.native_setting(svc::ID, key)).clone();
            // Listing the devices is slow: only for the device buttons.
            let (inputs, outputs) = if matches!(button, 4 | 5) {
                c.devices()
            } else {
                Default::default()
            };
            let reopen = settings.click(button, &inputs, &outputs);
            let stored = settings.stored();
            c.set_settings(settings, reopen);
            stored
        })
    };
    for (key, value) in stored {
        game.native_set_setting(svc::ID, key, &value);
    }
}
