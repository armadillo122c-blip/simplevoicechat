// The voice box: your microphone's state, your group, and who is talking.
fn svc_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    use horizon_client::extras::Corner;
    use simplevoicechat_engine as svc;
    if !game.native_on(svc::ID) || !game.native_corner_shown(Corner::TopLeft) {
        return;
    }
    let lines = svc::with_client(|c| c.hud());
    if lines.is_empty() {
        return;
    }
    let game = &*game;
    let w = lines
        .iter()
        .map(|l| game.native_text_width(&l.0))
        .fold(0.0f32, f32::max)
        + 6.0;
    let h = lines.len() as f32 * 10.0 + 4.0;
    let (x, y) = corners.place(Corner::TopLeft, w, h);
    game.native_fill(batch, [x, y, w, h], 0x7800_0000);
    for (i, (text, rgb)) in lines.iter().enumerate() {
        game.native_text(batch, text, [x + 3.0, y + 3.0 + i as f32 * 10.0], *rgb, true);
    }
}
