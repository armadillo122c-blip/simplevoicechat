// The window's tick: the microphone's frames go to the server.
fn svc_tick(game: &mut crate::game::Game<'_>) {
    use simplevoicechat_engine as svc;
    if !game.native_window() {
        return;
    }
    // Off in Options > Mods: as if outside a world, so the devices close.
    let listener = game
        .native_player()
        .filter(|_| game.native_on(svc::ID))
        .map(|p| svc::spatial::Listener {
            pos: [p.x, p.y + 1.62, p.z],
            yaw: p.yaw,
            dimension: p.dimension,
        });
    let (out, close) = {
        let game = &*game;
        svc::with_client(|c| {
            c.settings(|key| game.native_setting(svc::ID, key));
            (c.tick(listener), c.take_close())
        })
    };
    // The menu key on the menu: its key hook can't close the screen, so the tick does.
    if close {
        game.native_mod_screen_close(svc::ID);
    }
    for (channel, data) in out {
        // Fails until the player has joined, or on a server without mod messages: the client
        // says hello again until the server answers.
        let _ = game.native_net_send_to_server(channel, &data);
    }
}
