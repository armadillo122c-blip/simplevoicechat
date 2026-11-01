// A player's hello or microphone frames: forwarded at once to whoever hears them.
fn svc_from_client(game: &mut crate::game::Game<'_>, player: u128, channel: &str, data: &[u8]) {
    use simplevoicechat_engine as svc;
    if !channel.starts_with("simplevoicechat:") || !game.native_on(svc::ID) {
        return;
    }
    let out = svc::with_server(|s| {
        s.message(player, channel, data);
        s.drain()
    });
    for (to, channel, data) in out {
        let _ = game.native_net_send_to_player(to, channel, &data);
    }
}
