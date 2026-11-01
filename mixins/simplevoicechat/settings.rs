// Simple Voice Chat's settings page: activation, threshold, volumes, devices.
fn svc_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use simplevoicechat_engine as svc;
    if mod_id != svc::ID {
        return;
    }
    let game = &*game;
    let labels = svc::with_client(|c| c.settings(|key| game.native_setting(svc::ID, key)).labels());
    buttons.extend(labels);
}
