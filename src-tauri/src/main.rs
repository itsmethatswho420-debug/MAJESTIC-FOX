#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod profiles;
mod system_health;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            system_health::list_audio_endpoints,
            system_health::get_cpu_temp,
            profiles::list_profiles,
            profiles::get_profile,
            profiles::save_profile,
            profiles::delete_profile
        ])
        .run(tauri::generate_context!())
        .expect("error while running Majestic Fox");
}
