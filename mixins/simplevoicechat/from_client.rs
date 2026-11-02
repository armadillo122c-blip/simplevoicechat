// A player's hello, microphone frames or menu group action: frames are forwarded at once to
// whoever hears them, and a group action is answered in chat.
fn svc_from_client(game: &mut crate::game::Game<'_>, player: u128, channel: &str, data: &[u8]) {
    use simplevoicechat_engine as svc;
    if !channel.starts_with("simplevoicechat:") || !game.native_on(svc::ID) {
        return;
    }
    let (out, tells) = svc::with_server(|s| {
        s.message(player, channel, data);
        (s.drain(), s.drain_tells())
    });
    for (to, channel, data) in out {
        let _ = game.native_net_send_to_player(to, channel, &data);
    }
    for (to, line) in tells {
        game.native_tell(to, horizon_client::text::Text::literal(line));
    }
}
