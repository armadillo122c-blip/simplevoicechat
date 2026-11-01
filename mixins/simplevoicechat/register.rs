// /voicechat in every player's command tree.
fn svc_register_commands(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    for usage in simplevoicechat_engine::server::VoiceServer::COMMANDS {
        commands.push(crate::game::native_server::NativeCommand::new(usage));
    }
}
