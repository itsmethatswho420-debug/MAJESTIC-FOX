# Majestic Fox v0.4.0 — Build Notes (2026-10-14)

_The big batch. Everything Harry approved, one pass._

## What's new

### Rig Doctor (Health tab)
- Full machine diagnostics: CPU load %, per-core, memory, disk space, network throughput, CPU temperature (via LibreHardwareMonitor WMI).
- Plain-language diagnosis: what's choking the rig and exactly what to do about it. Nobody gets told "buy a better PC."
- New Rust module: `src-tauri/src/rig_doctor.rs` (`get_system_stats`, `diagnose_rig`). Uses `sysinfo` crate.
- Frontend: Rig Doctor section in HealthDashboard with diagnosis cards (OK / WATCH / FIX).

### Controller tab (new)
- Per-game full input-chain records: deadzones, aiming, layout, movement, Zen values (Harry only).
- Seeded with Harry's live BO7 numbers (2026-10-13). Little bro gets his own raw record.
- New: `src-tauri/src/controller.rs`, `src/components/ControllerTab.tsx`.

### Zen USB detection (Health tab)
- "Zen connected" banner when the DualSense fingerprint (VID_054C/PID_0CE6) is on USB.
- New command: `get_zen_presence` in `system_health.rs`.

### Cosmic-fox icon
- Real fox-spirit emblem (indigo-violet nebula, amber eyes) across all icon sizes + app header. Keyboard fox retired.

### Friendly device names
- Removed the temporary [RAW]/[property]/[registry] diagnostic tags from the Health tab.

### Chat tab (new)
- X mentions/replies in the dashboard. Polls every 2 min while live.
- **Cost note:** X is pay-per-use now (~$0.005/read). A 4-hour stream ≈ a few dollars in API credits. Token is Harry's own, stored locally.
- New: `src-tauri/src/xchat.rs`, `src/components/ChatPanel.tsx`. TikTok/Twitch chat join with the studio phase.

### Clips tab (new) — Clip-to-Social
- Recording picker, HTML5 player, Mark In/Out with frame nudge, multiple clips per recording.
- Export: ffmpeg trims to 1080×1920 vertical, burns in SRT captions, names `TikTok - YYYY-MM-DD - Title.mp4`.
- Per-clip Export tap = the approval gate. Nothing uploads or posts. Manual SRT upload for now; auto-transcription is a future hook.
- New: `src-tauri/src/clips.rs`, `src/components/ClipStudio.tsx`.
- **Dell needs ffmpeg:** nothing — the app downloads it itself on first export (~80 MB, one-time). Zero installs, zero commands.

## Verification
- `npx tsc --noEmit`: clean.
- ffmpeg pipeline smoke-tested (synthetic video → 1080×1920, exact trim, caption burn-in).
- Rust: Windows-only crate, no toolchain on this VM. Code follows existing module patterns. **Needs the Dell/CI build to confirm.**

## Deferred (per spec/scope)
- Auto-transcription, purple/dark grade, handle lower-third, auto-suggested highlights, posting queue destination handoffs.
- Thermal 45-min nudge timer (Rig Doctor covers the warnings).

## Companion deliverables (not in the app)
- Coach Phase 1 prototype: `../coach-phase1/` — muzzle-flash + drift detection, tested on Harry's clips, verdict: CONTINUE.
- Studio ingest research: `../studio-ingest-research.md` — Chiaki path ($0) + DNS relay path (PS5→TikTok, $5-6/mo VPS).
