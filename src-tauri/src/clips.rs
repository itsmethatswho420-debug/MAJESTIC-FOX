//! Clip-to-Social backend — studio-phase action (spec: `majestic-fox-clip-to-social-spec.md`).
//!
//! What this module does:
//!   1. Lists stream recordings (.mp4) from the app-data `recordings` dir,
//!      plus `~/workspace/stream_recordings/` as a dev fallback (that path
//!      only exists on the dev VM — on Harry's Dell only app-data applies).
//!   2. Exports a marked In/Out range as a 1080x1920 vertical clip via
//!      ffmpeg (center crop + scale, optional SRT burn-in), named with
//!      Harry's TikTok convention: `TikTok - YYYY-MM-DD - {title}.mp4`.
//!   3. Persists clip definitions (marks, titles, export status) in
//!      app-data `clips.json`.
//!
//! What it does NOT do (per spec, non-negotiable): nothing uploads or posts
//! anywhere. Export is local only. Posting-queue approvals and destination
//! handoffs live in the UI layer and in a later milestone — this module
//! never touches an account.
//!
//! Platform note: pure file IO + serde + shelling out to ffmpeg/ffprobe.
//! No Windows APIs — like profiles.rs, this compiles and runs anywhere on
//! purpose (Harry may one day run the clipper on the Lenovo too).

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use tauri::{command, AppHandle, Manager};

/// One stream recording found on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    /// File name, e.g. "20261013_221500.mp4".
    pub name: String,
    /// Absolute path, passed back to `export_clip` as `input_path`.
    pub path: String,
    /// File size in megabytes (1 decimal is plenty).
    pub size_mb: f64,
    /// Duration in seconds from ffprobe; 0 when ffprobe is unavailable or
    /// the file can't be probed.
    pub duration_sec: f64,
}

/// One marked clip: an In/Out range on a recording plus its export state.
///
/// `status` is one of: "draft" | "queued" | "exporting" | "done" | "error".
/// Transitions are driven by the frontend (the Export button); the backend
/// just stores whatever status it's given. Nothing auto-exports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    /// Stable id, e.g. "clip-1728854400000". Stamped by `save_clip` when empty.
    pub id: String,
    /// Absolute path of the source recording.
    pub recording_path: String,
    /// File name of the source recording (display only).
    pub recording_name: String,
    /// Mark-in, seconds into the recording.
    pub in_sec: f64,
    /// Mark-out, seconds into the recording. Must be > in_sec.
    pub out_sec: f64,
    /// Short title — becomes part of the exported file name.
    pub title: String,
    /// Export lifecycle state (see above).
    pub status: String,
    /// Absolute path of the exported clip, once `status` is "done".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    /// Human-readable failure reason, once `status` is "error".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// ISO timestamp of creation. Stamped by `save_clip` when empty.
    pub created_at: String,
}

/// On-disk shape of `clips.json`.
#[derive(Debug, Serialize, Deserialize)]
struct ClipsFile {
    #[serde(rename = "_comment", default, skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    comment: Option<String>,
    #[serde(default)]
    clips: Vec<Clip>,
}

/// Result of a successful `export_clip`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// Absolute path of the exported vertical clip.
    pub output_path: String,
    /// File name in Harry's TikTok convention — what gets copied to the
    /// Drive "TikTok - Ready to Post" folder later, by hand.
    pub drive_name: String,
    /// Clip duration in seconds (out - in).
    pub duration_sec: f64,
}

// ---------------------------------------------------------------------------
// Recordings
// ---------------------------------------------------------------------------

/// List every .mp4 recording, newest first. Reads the app-data `recordings`
/// dir and the dev fallback `~/workspace/stream_recordings/`. Missing dirs
/// are skipped silently — an empty list just means "no recordings here yet".
#[command]
pub fn list_recordings(app: AppHandle) -> Result<Vec<Recording>, String> {
    let mut out: Vec<(u64, Recording)> = Vec::new();

    for dir in recordings_dirs(&app) {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let is_mp4 = path
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("mp4"))
                .unwrap_or(false);
            if !is_mp4 || !path.is_file() {
                continue;
            }
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("recording.mp4")
                .to_string();
            out.push((
                modified_ms,
                Recording {
                    name,
                    path: path.to_string_lossy().to_string(),
                    size_mb: (meta.len() as f64 / 1_048_576.0 * 10.0).round() / 10.0,
                    duration_sec: probe_duration(&path),
                },
            ));
        }
    }

    // Newest first; de-dupe by absolute path (a recording could appear via
    // both dirs on a dev machine).
    out.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    Ok(out
        .into_iter()
        .map(|(_, r)| r)
        .filter(|r| seen.insert(r.path.clone()))
        .collect())
}

/// Probe a file's duration with ffprobe. Returns 0 when ffprobe is missing
/// or the file can't be probed — the frontend shows "—" for 0.
fn probe_duration(path: &PathBuf) -> f64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Directories scanned for recordings: app-data `recordings` (the real
/// inbox on Harry's machines) plus the dev fallback next to the workspace.
fn recordings_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(data) = app.path().app_data_dir() {
        dirs.push(data.join("recordings"));
    }
    if let Some(home) = home_dir() {
        let dev = home.join("workspace").join("stream_recordings");
        if dev.is_dir() {
            dirs.push(dev);
        }
    }
    dirs
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Clip metadata (clips.json)
// ---------------------------------------------------------------------------

/// List every saved clip definition, newest first.
#[command]
pub fn list_clips(app: AppHandle) -> Result<Vec<Clip>, String> {
    let mut clips = read_clips_file(&app)?;
    clips.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(clips)
}

/// Create or update a clip definition. Stamps `id` and `created_at` when
/// empty. Validates marks (0 <= in < out) and a non-empty title.
#[command]
pub fn save_clip(app: AppHandle, mut clip: Clip) -> Result<Clip, String> {
    validate_clip(&clip)?;
    if clip.id.trim().is_empty() {
        clip.id = format!("clip-{}", Local::now().timestamp_millis());
    }
    if clip.created_at.trim().is_empty() {
        clip.created_at = Local::now().to_rfc3339();
    }

    let mut stored = read_clips_file(&app)?;
    match stored.iter_mut().find(|c| c.id == clip.id) {
        Some(existing) => *existing = clip.clone(),
        None => stored.push(clip.clone()),
    }
    write_clips_file(&app, &stored)?;
    Ok(clip)
}

/// Delete a clip definition by id. This removes the definition only — the
/// source recording and any exported file stay on disk.
#[command]
pub fn delete_clip(app: AppHandle, id: String) -> Result<bool, String> {
    let mut stored = read_clips_file(&app)?;
    let before = stored.len();
    stored.retain(|c| c.id != id);
    if stored.len() == before {
        return Err(format!("no clip with id '{id}'"));
    }
    write_clips_file(&app, &stored)?;
    Ok(true)
}

fn validate_clip(c: &Clip) -> Result<(), String> {
    if c.in_sec < 0.0 {
        return Err("mark-in can't be negative".to_string());
    }
    if c.out_sec <= c.in_sec {
        return Err("mark-out must be after mark-in".to_string());
    }
    if c.title.trim().is_empty() {
        return Err("the clip needs a title".to_string());
    }
    if c.recording_path.trim().is_empty() {
        return Err("the clip needs a source recording".to_string());
    }
    Ok(())
}

fn clips_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("clips.json"))
}

fn read_clips_file(app: &AppHandle) -> Result<Vec<Clip>, String> {
    let path = match clips_file(app) {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let file: ClipsFile = serde_json::from_str(&raw)
        .map_err(|e| format!("could not parse {}: {e}", path.display()))?;
    Ok(file.clips)
}

fn write_clips_file(app: &AppHandle, clips: &[Clip]) -> Result<(), String> {
    let path = clips_file(app).ok_or_else(|| "could not resolve the app-data directory".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let file = ClipsFile {
        comment: Some(
            "Majestic Fox clip definitions (Clip-to-Social). Saved from the app's Clips tab. \
             Safe to back up; the app re-reads this file on every list."
                .to_string(),
        ),
        clips: clips.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|e| format!("could not serialize clips: {e}"))?;
    std::fs::write(&path, raw).map_err(|e| format!("could not write {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Export one clip: trim in/out, center-crop + scale to 1080x1920, burn in
/// an SRT caption track when `captions_srt` is provided.
///
/// `captions_srt` is the SRT file *content* (the frontend reads the uploaded
/// .srt into text) — the backend writes it next to the output as
/// `<stem>.srt` and references it from the subtitles filter.
///
/// Returns a clear error when ffmpeg is missing instead of failing cryptically.
/// On first use the app fetches ffmpeg itself — Harry never installs anything.
#[command]
pub fn export_clip(
    app: AppHandle,
    input_path: String,
    in_sec: f64,
    out_sec: f64,
    title: String,
    captions_srt: Option<String>,
) -> Result<ExportResult, String> {
    let ffmpeg = ensure_ffmpeg(&app)?;

    let input = PathBuf::from(&input_path);
    if !input.is_file() {
        return Err(format!("recording not found: {input_path}"));
    }
    if in_sec < 0.0 || out_sec <= in_sec {
        return Err("mark-out must be after mark-in".to_string());
    }
    let safe_title = sanitize_title(&title);
    if safe_title.is_empty() {
        return Err("the clip needs a title".to_string());
    }

    let dir = clips_dir(&app).ok_or_else(|| "could not resolve the app-data directory".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    // Harry's TikTok convention: `TikTok - YYYY-MM-DD - {title}.mp4`.
    let date = Local::now().format("%Y-%m-%d").to_string();
    let drive_name = format!("TikTok - {date} - {safe_title}.mp4");
    let output = unique_path(&dir, &drive_name);

    // Optional caption burn-in: write the SRT next to the output.
    let srt_path: Option<PathBuf> = match captions_srt {
        Some(text) if !text.trim().is_empty() => {
            let p = output.with_extension("srt");
            std::fs::write(&p, text)
                .map_err(|e| format!("could not write caption file {}: {e}", p.display()))?;
            Some(p)
        }
        _ => None,
    };

    // Center-crop the landscape frame to 9:16, then scale to 1080x1920.
    // trunc(/2)*2 keeps dimensions even (x264 requirement).
    let mut vf = "crop=trunc(ih*9/16/2)*2:trunc(ih/2)*2,scale=1080:1920".to_string();
    if let Some(srt) = &srt_path {
        vf.push_str(&format!(",subtitles='{}'", escape_subtitles_path(srt)));
    }

    let status = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-ss")
        .arg(format!("{in_sec:.3}"))
        .arg("-to")
        .arg(format!("{out_sec:.3}"))
        .arg("-i")
        .arg(&input)
        .arg("-vf")
        .arg(&vf)
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("veryfast")
        .arg("-crf")
        .arg("20")
        .arg("-c:a")
        .arg("aac")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&output)
        .output()
        .map_err(|e| format!("could not run ffmpeg: {e}"))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
        return Err(format!(
            "ffmpeg failed: {}",
            tail.into_iter().rev().collect::<Vec<_>>().join(" / ")
        ));
    }

    Ok(ExportResult {
        output_path: output.to_string_lossy().to_string(),
        drive_name: output
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&drive_name)
            .to_string(),
        duration_sec: (out_sec - in_sec).max(0.0),
    })
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Where the app keeps its own ffmpeg copy.
fn bundled_ffmpeg(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("bin").join("ffmpeg.exe"))
}

/// Resolve ffmpeg: app's own copy first, then PATH. On first use — when
/// neither exists — download the Windows essentials build into the app-data
/// dir automatically. Harry never touches an installer or a command line.
fn ensure_ffmpeg(app: &AppHandle) -> Result<PathBuf, String> {
    // 1. Our own copy from a previous run.
    if let Some(p) = bundled_ffmpeg(app) {
        if p.is_file() {
            return Ok(p);
        }
    }
    // 2. System PATH (in case he already has it).
    if ffmpeg_available() {
        return Ok(PathBuf::from("ffmpeg"));
    }
    // 3. Fetch it ourselves.
    download_ffmpeg(app)
}

/// Download the ffmpeg Windows essentials build and extract ffmpeg.exe
/// into the app-data bin dir. PowerShell handles download + unzip natively.
#[cfg(target_os = "windows")]
fn download_ffmpeg(app: &AppHandle) -> Result<PathBuf, String> {
    use std::process::Command;

    let bin_dir = app
        .path()
        .app_data_dir()
        .map(|d| d.join("bin"))
        .map_err(|e| format!("could not resolve app-data dir: {e}"))?;
    std::fs::create_dir_all(&bin_dir)
        .map_err(|e| format!("could not create {}: {e}", bin_dir.display()))?;

    let dest = bin_dir.join("ffmpeg.exe");
    let zip = bin_dir.join("ffmpeg-dl.zip");
    let zip_s = zip.to_string_lossy().replace('\'', "''");
    let dest_s = dest.to_string_lossy().replace('\'', "''");
    let bin_s = bin_dir.to_string_lossy().replace('\'', "''");

    // ~80 MB download, one-time. gyan.dev release essentials.
    let ps = format!(
        "$ProgressPreference='SilentlyContinue'; \
         Invoke-WebRequest -Uri 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip' \
           -OutFile '{zip_s}' -UseBasicParsing; \
         Expand-Archive -Path '{zip_s}' -DestinationPath '{bin_s}\\extracted' -Force; \
         $exe = Get-ChildItem -Path '{bin_s}\\extracted' -Recurse -Filter 'ffmpeg.exe' | Select-Object -First 1; \
         Move-Item -Path $exe.FullName -Destination '{dest_s}' -Force; \
         Remove-Item -Recurse -Force '{bin_s}\\extracted'; \
         Remove-Item -Force '{zip_s}'"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
        .map_err(|e| format!("ffmpeg download failed: {e}"))?;
    if !out.status.success() || !dest.is_file() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "could not fetch ffmpeg automatically: {}. \
             Manual fallback: winget install Gyan.FFmpeg, then export again.",
            err.trim().chars().take(200).collect::<String>()
        ));
    }
    Ok(dest)
}

#[cfg(not(target_os = "windows"))]
fn download_ffmpeg(_app: &AppHandle) -> Result<PathBuf, String> {
    Err("automatic ffmpeg download is implemented for Windows only.".to_string())
}

fn clips_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("clips"))
}

/// Strip characters that are illegal in Windows file names (and path
/// separators everywhere), collapse whitespace, cap length. The result is
/// safe to embed in `TikTok - YYYY-MM-DD - {title}.mp4`.
fn sanitize_title(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(60).collect::<String>().trim().to_string()
}

/// If `<dir>/<name>` exists, try `<name> (2)`, `<name> (3)`, … — exports
/// never overwrite.
fn unique_path(dir: &PathBuf, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = name.trim_end_matches(".mp4");
    for n in 2..1000 {
        let p = dir.join(format!("{stem} ({n}).mp4"));
        if !p.exists() {
            return p;
        }
    }
    candidate
}

/// Escape a file path for ffmpeg's subtitles filter: forward slashes, then
/// escape `:`, `'`, and `[`/`]`-adjacent specials the filter would parse.
fn escape_subtitles_path(path: &PathBuf) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}
