// The end of each server tick: where everyone stands, who left, and the players' lists.
fn svc_server_tick(
    game: &mut crate::game::Game<'_>,
    phase: crate::game::native_lifecycle::ServerTickPhase,
) {
    use simplevoicechat_engine::{self as svc, server::{Config, Player}};
    if !matches!(
        phase,
        crate::game::native_lifecycle::ServerTickPhase::ServerTickEnd { .. }
    ) || !game.native_on(svc::ID)
    {
        return;
    }
    let players: Vec<Player> = game
        .native_server_players()
        .into_iter()
        .map(|p| Player {
            uuid: p.uuid,
            name: p.name,
            dimension: p.dimension,
            position: p.position,
        })
        .collect();
    // The host's own choice; whispers carry half as far, as Simple Voice Chat's default.
    let distance = game
        .native_setting(svc::ID, "distance")
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(48.0)
        .clamp(1.0, 512.0);
    let out = svc::with_server(|s| {
        s.config = Config {
            distance,
            whisper: distance / 2.0,
        };
        s.tick(players);
        s.drain()
    });
    for (to, channel, data) in out {
        let _ = game.native_net_send_to_player(to, channel, &data);
    }
}
