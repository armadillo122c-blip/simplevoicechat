// A character typed on the voice chat menu: into its text box.
fn svc_menu_char(game: &crate::game::Game<'_>, c: char, canceled: &mut bool) {
    use simplevoicechat_engine as svc;
    if game.native_mod_screen() == Some(svc::ID) && svc::with_client(|v| v.menu_char(c)) {
        *canceled = true;
    }
}