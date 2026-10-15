//! Controller tab — the full input chain at record time.
//!
//! Why this exists: the coach can't do clean math on recoil/drift data
//! unless it knows EVERYTHING between Harry's thumb and the game — CoD
//! deadzones, sensitivity, response curve, button layout, PLUS his Zen
//! values. Little bro logs CoD settings only (raw, no Zen); Harry logs
//! CoD settings + Zen values. Type it once — settings barely change.
//!
//! Records are per-game (BO7 template now, MW4 template at launch — his
//! numbers never mix). Storage mirrors `profiles.rs`: JSON in the app-data
//! dir. Photo of the settings screen works as personal reference; the
//! typed record is what the coach reads.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{command, AppHandle, Manager};

/// Deadzone inputs — the numbers that shape the stick before anything else.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeadzoneInputs {
    pub left_stick_min: u8,
    pub left_stick_max: u8,
    pub right_stick_min: u8,
    pub right_stick_max: u8,
    pub l2_deadzone: u8,
    pub r2_deadzone: u8,
}

/// Core aiming settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AimingSettings {
    pub horiz_sensitivity: u8,
    pub vert_sensitivity: u8,
    /// "Standard" | "Linear" | "Dynamic"
    pub response_curve: String,
    pub curve_slope_scale: f32,
    pub ads_sensitivity_multiplier: f32,
    pub target_aim_assist: bool,
    pub ads_aim_assist: bool,
    /// "Gradual" | "Instant"
    pub ads_transition_timing: String,
    pub look_inversion: bool,
}

/// Button/layout settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSettings {
    /// "Default" | "Tactical" | "Default Flipped"
    pub button_layout: String,
    pub stick_layout_preset: String,
    pub controller_vibration: bool,
    /// "Off" | "Full" etc.
    pub trigger_effect: String,
    pub bumper_ping: bool,
    pub swap_bumpers_triggers: bool,
}

/// Movement settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MovementSettings {
    pub auto_sprint: bool,
    pub slide_behavior: String,
    pub corner_slice: bool,
    pub notes: String,
}

/// Harry's Zen-side values for this game. None for little bro (raw).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ZenValues {
    pub antirecoil_vertical: u8,
    pub antirecoil_horizontal: u8,
    pub script_curve: String,
    pub script_slot: String,
    pub notes: String,
}

/// One controller record: the full input chain for a game at record time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerRecord {
    /// Stable slug, e.g. "bo7-harry".
    pub id: String,
    /// "BO7" | "MW4" | ...
    pub game: String,
    /// "harry" (Zen on) | "little-bro" (raw, no Zen).
    pub player: String,
    pub recorded_at: String,
    pub deadzone: DeadzoneInputs,
    pub aiming: AimingSettings,
    pub layout: LayoutSettings,
    pub movement: MovementSettings,
    /// None for little bro.
    pub zen: Option<ZenValues>,
    pub notes: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ControllerFile {
    #[serde(default)]
    records: Vec<ControllerRecord>,
}

fn controller_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("controller.json"))
        .map_err(|e| format!("could not resolve app-data dir: {e}"))
}

fn load_records(app: &AppHandle) -> Vec<ControllerRecord> {
    let path = match controller_path(app) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    if !path.exists() {
        return seed_records();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<ControllerFile>(&s).ok())
        .map(|f| f.records)
        .unwrap_or_default()
}

/// Seed with Harry's live BO7 numbers (captured 2026-10-13) so the tab
/// isn't empty on first open. He corrects from here.
fn seed_records() -> Vec<ControllerRecord> {
    vec![ControllerRecord {
        id: "bo7-harry".to_string(),
        game: "BO7".to_string(),
        player: "harry".to_string(),
        recorded_at: "2026-10-13".to_string(),
        deadzone: DeadzoneInputs {
            left_stick_min: 3,
            left_stick_max: 41,
            right_stick_min: 13,
            right_stick_max: 99,
            l2_deadzone: 0,
            r2_deadzone: 0,
        },
        aiming: AimingSettings {
            horiz_sensitivity: 165,
            vert_sensitivity: 160,
            response_curve: "Linear".to_string(),
            curve_slope_scale: 0.80,
            ads_sensitivity_multiplier: 0.95,
            target_aim_assist: true,
            ads_aim_assist: false,
            ads_transition_timing: "Gradual".to_string(),
            look_inversion: false,
        },
        layout: LayoutSettings {
            button_layout: "Default".to_string(),
            stick_layout_preset: "Default".to_string(),
            controller_vibration: false,
            trigger_effect: "Full".to_string(),
            bumper_ping: false,
            swap_bumpers_triggers: false,
        },
        movement: MovementSettings {
            auto_sprint: false,
            slide_behavior: "".to_string(),
            corner_slice: false,
            notes: "".to_string(),
        },
        zen: Some(ZenValues {
            antirecoil_vertical: 39,
            antirecoil_horizontal: 0,
            script_curve: "aggressive-1.15x-down/1.0x-up".to_string(),
            script_slot: "5".to_string(),
            notes: "Fox_V2_Harry_Aggressive.gpc (5) — asymmetric vertical curve, in testing 2026-10-15".to_string(),
        }),
        notes: "Harry's live values captured from his screen 2026-10-15. Extra CoD settings (combat, movement behaviors, tactical sprint, etc.) go in Notes until structured fields are added.".to_string(),
    }]
}

#[command]
pub fn list_controller_records(app: AppHandle) -> Result<Vec<ControllerRecord>, String> {
    Ok(load_records(&app))
}

#[command]
pub fn save_controller_record(
    app: AppHandle,
    record: ControllerRecord,
) -> Result<Vec<ControllerRecord>, String> {
    let path = controller_path(&app)?;
    let mut records = load_records(&app);
    if let Some(i) = records.iter().position(|r| r.id == record.id) {
        records[i] = record;
    } else {
        records.push(record);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create app-data dir: {e}"))?;
    }
    let file = ControllerFile { records: records.clone() };
    let json = serde_json::to_string_pretty(&file).map_err(|e| format!("serialize failed: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write failed: {e}"))?;
    Ok(records)
}

#[command]
pub fn delete_controller_record(
    app: AppHandle,
    id: String,
) -> Result<Vec<ControllerRecord>, String> {
    let path = controller_path(&app)?;
    let mut records = load_records(&app);
    records.retain(|r| r.id != id);
    let file = ControllerFile { records: records.clone() };
    let json = serde_json::to_string_pretty(&file).map_err(|e| format!("serialize failed: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write failed: {e}"))?;
    Ok(records)
}
