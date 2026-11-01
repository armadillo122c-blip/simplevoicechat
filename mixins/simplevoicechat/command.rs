// A player's /voicechat: groups and help.
fn svc_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    use horizon_client::text::Text;
    use simplevoicechat_engine as svc;
    if *ran || command.split_whitespace().next() != Some("voicechat") {
        return;
    }
    *ran = true;
    let (reply, others) = svc::with_server(|s| s.command(player.uuid, command));
    for line in reply {
        game.native_feedback(Text::literal(line));
    }
    for (to, line) in others {
        game.native_tell(to, Text::literal(line));
    }
}
