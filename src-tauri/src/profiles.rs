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
    /// Where this entry came from: "seed" (bundled, read-only) or "user"
    /// (Harry's own, saved in the app-data dir). Never read from disk —
    /// always stamped at list time so a hand-edited file can't lie about it.
    #[serde(default, skip_serializing)]
    pub source: String,
}

/// On-disk shape of a profiles file: seed bundle and user file alike.
/// `_comment` is documentation only. `source` is never stored (see above).
#[derive(Debug, Serialize, Deserialize)]
struct ProfilesFile {
    #[serde(rename = "_comment", default, skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    comment: Option<String>,
    #[serde(default)]
    profiles: Vec<Profile>,
}

/// List every profile: user profiles first (when that file exists), then
/// the bundled/dev seeds. User entries win on id collision. Each entry is
/// stamped with its `source` ("user" / "seed") at list time.
#[command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<Profile>, String> {
    let mut profiles: Vec<Profile> = Vec::new();

    // 1. User profiles. Read-write as of v0.2.0 — this is where Harry's own
    //    saved profiles live.
    if let Some(user_file) = user_profiles_file(&app) {
        if user_file.exists() {
            let mut user = read_profiles_file(&user_file)?;
            for p in &mut user {
                p.source = "user".to_string();
            }
            profiles.extend(user);
        }
    }

    // 2/3. Seed profiles (bundled resource, then dev fallback).
    let seed = read_profiles_file(&seed_profiles_path(&app)?)?;
    for mut p in seed {
        if !profiles.iter().any(|u| u.id == p.id) {
            p.source = "seed".to_string();
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

/// Save (create or update) one of Harry's own profiles. Seed profiles are
/// never modified in place: saving an edit to a seed id writes a user-layer
/// override with the same id, which wins at list time. Returns the saved
/// profile stamped `source: "user"`.
#[command]
pub fn save_profile(app: AppHandle, mut profile: Profile) -> Result<Profile, String> {
    validate_profile(&profile)?;
    if profile.game.trim().is_empty() {
        return Err("profile needs a game name (e.g. \"Warzone\")".to_string());
    }
    if profile.weapon.trim().is_empty() {
        return Err("profile needs a weapon name (e.g. \"Assault Rifle\")".to_string());
    }

    let user_file = user_profiles_file(&app)
        .ok_or_else(|| "could not resolve the app-data directory".to_string())?;
    if let Some(parent) = user_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }

    let mut stored = if user_file.exists() {
        read_profiles_file(&user_file)?
    } else {
        Vec::new()
    };

    profile.source = "user".to_string();
    match stored.iter_mut().find(|p| p.id == profile.id) {
        Some(existing) => *existing = profile.clone(),
        None => stored.push(profile.clone()),
    }

    write_profiles_file(&user_file, &stored)?;
    Ok(profile)
}

/// Delete one of Harry's own profiles. Built-in seed profiles cannot be
/// deleted (they ship with the app); deleting a user override of a seed id
/// restores the seed's original values at the next list.
#[command]
pub fn delete_profile(app: AppHandle, id: String) -> Result<bool, String> {
    let user_file = user_profiles_file(&app)
        .ok_or_else(|| "could not resolve the app-data directory".to_string())?;
    let mut stored = if user_file.exists() {
        read_profiles_file(&user_file)?
    } else {
        Vec::new()
    };

    let before = stored.len();
    stored.retain(|p| p.id != id);
    if stored.len() == before {
        return Err(format!(
            "'{id}' is a built-in seed profile and cannot be deleted. \
             Edit it instead — your edit is saved as your own override."
        ));
    }

    write_profiles_file(&user_file, &stored)?;
    Ok(true)
}

/// Write the user profiles file (pretty JSON, so it's human-readable if
/// Harry ever opens it).
fn write_profiles_file(path: &PathBuf, profiles: &[Profile]) -> Result<(), String> {
    let file = ProfilesFile {
        comment: Some(
            "Harry's own Majestic Fox profiles. Saved from the app's Profiles tab. \
             Safe to back up; the app re-reads this file on every list."
                .to_string(),
        ),
        profiles: profiles.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("could not serialize profiles: {e}"))?;
    std::fs::write(path, raw).map_err(|e| format!("could not write {}: {e}", path.display()))
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
    let file: ProfilesFile = serde_json::from_str(&raw)
        .map_err(|e| format!("could not parse {}: {e}", path.display()))?;
    for p in &file.profiles {
        validate_profile(p)?;
    }
    Ok(file.profiles)
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
