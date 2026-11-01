// Simple Voice Chat's row in Options > Mods, with its keys.
fn svc_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use simplevoicechat_engine as svc;
    mods.push(crate::game::native_hooks::NativeMod {
        id: svc::ID.into(),
        title: format!("{} {}", svc::TITLE, svc::VERSION),
        description: svc::DESCRIPTION.into(),
        keys: svc::client::KEYS
            .iter()
            .map(|k| [k[0].into(), k[1].into(), k[2].into()])
            .collect(),
    });
}
