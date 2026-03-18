pub mod audio;
pub mod commands;
pub mod http_client;
pub mod state;
pub mod transport;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "walkietalk_client_lib=debug,info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::auth::login,
            commands::auth::register,
            commands::auth::logout,
            commands::auth::get_current_user,
            commands::rooms::get_rooms,
            commands::rooms::create_room,
            commands::rooms::join_by_code,
            commands::rooms::join_room,
            commands::rooms::leave_room,
            commands::rooms::get_public_rooms,
            commands::rooms::get_room_settings,
            commands::rooms::update_room,
            commands::rooms::delete_room,
            commands::rooms::regenerate_invite,
            commands::connection::connect,
            commands::connection::disconnect,
            commands::connection::reconnect,
            commands::realtime::join_room_ws,
            commands::realtime::leave_room_ws,
            commands::floor::request_floor,
            commands::floor::release_floor,
            commands::misc::trigger_haptic,
            commands::misc::play_sound,
            commands::settings::get_server_url,
            commands::settings::set_server_url,
            commands::audio::start_audio_capture,
            commands::audio::stop_audio_capture,
        ])
        .run(tauri::generate_context!())
        .expect("error while running WalkieTalk");
}
