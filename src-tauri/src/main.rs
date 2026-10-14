#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clips;
mod controller;
mod profiles;
mod rig_doctor;
mod system_health;
mod xchat;
mod zen_export;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            system_health::list_audio_endpoints,
            system_health::get_cpu_temp,
            system_health::get_zen_presence,
            rig_doctor::get_system_stats,
            rig_doctor::diagnose_rig,
            controller::list_controller_records,
            controller::save_controller_record,
            controller::delete_controller_record,
            xchat::x_resolve_user,
            xchat::x_poll_mentions,
            profiles::list_profiles,
            profiles::get_profile,
            profiles::save_profile,
            profiles::delete_profile,
            zen_export::export_zen_gpc,
            clips::list_recordings,
            clips::list_clips,
            clips::save_clip,
            clips::delete_clip,
            clips::export_clip
        ])
        .run(tauri::generate_context!())
        .expect("error while running Majestic Fox");
}
