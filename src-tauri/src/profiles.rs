//! Profile library for the "One script, many profiles" architecture.
//!
//! The Zen runs ONE script (Harry's Fox custom). This app holds the profile
//! library: one profile per weapon/game with that weapon's tuning values.
//! Pushing values to the Zen is a later milestone (see TODO in
//! `ProfileManager.tsx`) — for now this module is the read side: list and
//! fetch profiles from disk.
//!
//! Lookup order:
//!   1. User profiles file (`profiles.json` in the app-data dir) — the
//!      future home of Harry's own saved profiles. Not written yet.
//!   2. Bundled seed file (`profiles/seed-profiles.json`, shipped via
//!      `bundle.resources`) — the examples that ship with the app.
//!   3. Dev fallback: `src-tauri/profiles/seed-profiles.json` next to this
//!      crate (so `tauri dev` works without a packaged build).
//!
//! Pure file IO + serde. No Windows APIs, no COM — this compiles and runs
//! anywhere, which is deliberate: profile management must never depend on
//! the machine being the Dell.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{command, AppHandle, Manager};

/// Tuning values for one weapon. The anti-recoil fields use the same 0-100
/// scale as the Zen quick-nudge (hold ADS + D-pad on Harry's Fox custom),
/// so a profile's numbers can be dialed in by hand today and pushed
/// automatically later.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileValues {
    /// Vertical anti-recoil strength, 0-100.
    pub antirecoil_vertical: u8,
    /// Horizontal anti-recoil strength, 0-100.
    pub antirecoil_horizontal: u8,
    /// Stick deadzone, 0-100.
    pub stick_deadzone: u8,
    /// Stick response curve name, e.g. "linear" | "dynamic" | "aggressive".
    pub response_curve: String,
}

/// One profile: a named set of tuning values for a weapon in a game.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Stable slug, e.g. "mw4-assault-rifle".
    pub id: String,
    /// Game this profile is tuned for, e.g. "MW4".
    pub game: String,
    /// Weapon this profile is tuned for, e.g. "Assault Rifle".
    pub weapon: String,
    /// Human notes: what this profile is, that it's an example, how to tune.
    pub notes: String,
    pub values: ProfileValues,
}

#[derive(Debug, Deserialize)]
struct SeedFile {
    /// Ignored at runtime; documents that the seeds are examples.
    #[allow(dead_code)]
    _comment: Option<String>,
    profiles: Vec<Profile>,
}

/// List every profile: user profiles first (when that file exists), then
/// the bundled/dev seeds. User entries win on id collision.
#[command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<Profile>, String> {
    let mut profiles: Vec<Profile> = Vec::new();

    // 1. User profiles (future). Read-only for now — nothing writes this
    //    file yet; when saving lands, it goes here.
    if let Some(user_file) = user_profiles_file(&app) {
        if user_file.exists() {
            let user = read_profiles_file(&user_file)?;
            profiles.extend(user);
        }
    }

    // 2/3. Seed profiles (bundled resource, then dev fallback).
    let seed = read_profiles_file(&seed_profiles_path(&app)?)?;
    for p in seed {
        if !profiles.iter().any(|u| u.id == p.id) {
            profiles.push(p);
        }
    }

    Ok(profiles)
}

/// Fetch one profile by id. Returns `Ok(None)` when the id is unknown.
#[command]
pub fn get_profile(app: AppHandle, id: String) -> Result<Option<Profile>, String> {
    Ok(list_profiles(app)?.into_iter().find(|p| p.id == id))
}

/// Future home of Harry's own saved profiles:
/// `<app-data>/profiles.json`. Returns `None` when the app-data dir
/// cannot be resolved (never an error — we just skip the user layer).
fn user_profiles_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("profiles.json"))
}

/// Locate the seed file: bundled resource first, then the dev fallback
/// next to this crate.
fn seed_profiles_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(res_dir) = app.path().resource_dir() {
        let bundled = res_dir.join("profiles").join("seed-profiles.json");
        if bundled.exists() {
            return Ok(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("profiles")
        .join("seed-profiles.json");
    if dev.exists() {
        return Ok(dev);
    }
    Err(
        "seed profiles file not found (checked bundled resources and src-tauri/profiles/)"
            .to_string(),
    )
}

/// Read + validate a profiles file. Every numeric tuning value must sit in
/// 0-100; violations name the profile and field so a bad file is debuggable
/// instead of silently clamped.
fn read_profiles_file(path: &PathBuf) -> Result<Vec<Profile>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let seed: SeedFile = serde_json::from_str(&raw)
        .map_err(|e| format!("could not parse {}: {e}", path.display()))?;
    for p in &seed.profiles {
        validate_profile(p)?;
    }
    Ok(seed.profiles)
}

fn validate_profile(p: &Profile) -> Result<(), String> {
    let v = &p.values;
    let mut bad: Vec<&str> = Vec::new();
    if v.antirecoil_vertical > 100 {
        bad.push("antirecoilVertical");
    }
    if v.antirecoil_horizontal > 100 {
        bad.push("antirecoilHorizontal");
    }
    if v.stick_deadzone > 100 {
        bad.push("stickDeadzone");
    }
    if p.id.trim().is_empty() {
        bad.push("id (empty)");
    }
    if !bad.is_empty() {
        return Err(format!(
            "profile '{}' has invalid field(s): {} (numeric values must be 0-100)",
            p.id,
            bad.join(", ")
        ));
    }
    Ok(())
}
