// The voice chat menu, drawn over the world while it is open.
fn svc_menu_draw(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
) {
    use simplevoicechat_engine::{
        self as svc,
        menu::{self, Shape},
    };
    if game.native_mod_screen() != Some(svc::ID) {
        return;
    }
    let view = svc::with_client(|c| c.menu_view());
    let layout = menu::layout(&view, screen.rect, screen.mouse, &|t| game.native_text_width(t));
    for shape in layout.shapes {
        match shape {
            Shape::Fill(r, argb) => game.native_fill(batch, r, argb),
            Shape::Text(text, at, rgb) => game.native_text(batch, &text, at, rgb, true),
            Shape::Sprite(name, src, dst) => {
                if !game.native_sprite(batch, name, src, dst) {
                    game.native_fill(batch, dst, 0xFF55_5555);
                }
            }
        }
    }
}