// A voice or the server's list of players with voice chat.
fn svc_from_server(game: &mut crate::game::Game<'_>, channel: &str, data: &[u8]) {
    use simplevoicechat_engine as svc;
    if !channel.starts_with("simplevoicechat:") || !game.native_on(svc::ID) {
        return;
    }
    let listener = game.native_player().map(|p| svc::spatial::Listener {
        pos: [p.x, p.y + 1.62, p.z],
        yaw: p.yaw,
        dimension: p.dimension,
    });
    svc::with_client(|c| c.message(channel, data, listener));
}
