//! Export to Zen — render Majestic Fox profiles into a ready-to-compile
//! Fox V3 `.gpc` script.
//!
//! Honest scope: the app cannot push values into a running Zen script
//! (GPC has no PC-to-script data channel). What it CAN do is bake the
//! profile values into a clean, readable script file that Harry compiles
//! and programs through Zen Studio's normal flow. The template lives in
//! `zen/fox_v3.gpc.tpl` and is embedded at compile time.
//!
//! One template serves both cases: exporting one profile renders it with a
//! single preset; exporting all renders every profile as a switchable
//! preset (hold VIEW + D-pad LEFT/RIGHT on the device).

use super::profiles::list_profiles;
use tauri::{command, AppHandle, Manager};

/// The Fox V3 template, embedded at compile time so the export works
/// without any runtime resource lookup.
const TEMPLATE: &str = include_str!("../zen/fox_v3.gpc.tpl");

/// Stable export folder under the user's Documents.
const EXPORT_DIR_NAME: &str = "Majestic Fox/Zen Exports";

/// Export profiles to a Fox V3 `.gpc` file.
///
/// - `profile_id = Some(id)`: single-profile export (one preset).
/// - `profile_id = None`: every profile becomes a switchable preset.
///
/// Returns the absolute path of the written file. The filename is stable
/// (`FoxV3_Profile.gpc` / `FoxV3_AllProfiles.gpc`) so Harry's Zen Studio
/// always opens the same path.
#[command]
pub fn export_zen_gpc(app: AppHandle, profile_id: Option<String>) -> Result<String, String> {
    let all = list_profiles(app.clone())?;
    if all.is_empty() {
        return Err("no profiles to export — add one on the Profiles tab first".to_string());
    }

    let selected: Vec<_> = match profile_id {
        Some(id) => {
            let found: Vec<_> = all.into_iter().filter(|p| p.id == id).collect();
            if found.is_empty() {
                return Err(format!("profile '{id}' not found"));
            }
            found
        }
        None => all,
    };

    let rendered = render_template(&selected)?;
    let path = export_path(&app, selected.len() == 1)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, rendered)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;

    Ok(path.to_string_lossy().to_string())
}

/// Fill the template placeholders from the selected profiles.
fn render_template(
    profiles: &[super::profiles::Profile],
) -> Result<String, String> {
    if profiles.is_empty() {
        return Err("nothing selected to export".to_string());
    }

    let names: Vec<String> = profiles
        .iter()
        .map(|p| format!("\"{}\"", gpc_string(&format!("{} {}", p.game, p.weapon))))
        .collect();
    let vs: Vec<String> = profiles.iter().map(|p| p.values.antirecoil_vertical.to_string()).collect();
    let hs: Vec<String> = profiles
        .iter()
        .map(|p| p.values.antirecoil_horizontal.to_string())
        .collect();
    let dzs: Vec<String> = profiles.iter().map(|p| p.values.stick_deadzone.to_string()).collect();
    let curves: Vec<String> = profiles
        .iter()
        .map(|p| curve_kind(&p.values.response_curve).to_string())
        .collect();
    let source_names = profiles
        .iter()
        .map(|p| format!("{} — {}", p.game, p.weapon))
        .collect::<Vec<_>>()
        .join("; ");

    // Timestamp without external crates: seconds since the Unix epoch.
    let generated_at = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("{} (unix epoch seconds)", d.as_secs()),
        Err(_) => "unknown".to_string(),
    };

    let out = TEMPLATE
        .replace("{{PRESET_COUNT}}", &profiles.len().to_string())
        .replace("{{PRESET_NAMES}}", &names.join(", "))
        .replace("{{PRESET_V}}", &vs.join(", "))
        .replace("{{PRESET_H}}", &hs.join(", "))
        .replace("{{PRESET_DZ}}", &dzs.join(", "))
        .replace("{{PRESET_CURVE}}", &curves.join(", "))
        .replace("{{DEFAULT_PRESET}}", "0")
        .replace("{{GENERATED_AT}}", &generated_at)
        .replace("{{SOURCE_NAMES}}", &source_names);

    if out.contains("{{") {
        return Err("template rendering left placeholders behind — aborting".to_string());
    }
    Ok(out)
}

/// Make a string safe for a GPC string literal: strip quotes/backslashes,
/// collapse whitespace, cap length for the little OLED.
fn gpc_string(raw: &str) -> String {
    raw.replace(['"', '\\'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(24)
        .collect()
}

/// Map the app's curve name to the template's curve kind.
fn curve_kind(name: &str) -> u8 {
    match name.trim().to_lowercase().as_str() {
        "dynamic" => 1,
        "aggressive" => 2,
        _ => 0, // linear and anything unrecognized
    }
}

/// Stable export path: Documents/Majestic Fox/Zen Exports/<file>.gpc
fn export_path(app: &AppHandle, single: bool) -> Result<std::path::PathBuf, String> {
    let docs = app
        .path()
        .document_dir()
        .map_err(|_| "could not locate your Documents folder".to_string())?;
    let file = if single {
        "FoxV3_Profile.gpc"
    } else {
        "FoxV3_AllProfiles.gpc"
    };
    Ok(docs.join(EXPORT_DIR_NAME).join(file))
}
