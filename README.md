# Majestic Fox — desktop app

**Muse co-pilot for Harry's stream rig.** Phase 1 is the stream co-pilot: a
pre-stream rig-health check that verifies every audio device is running at
**48000 Hz** (the sync-drift fix for the Dell) and takes a CPU temperature
reading — plus the **profile library** for the "One script, many profiles"
architecture: the Zen runs Harry's one Fox custom script, and the app holds
one tuning profile per weapon/game. Featherweight by design — Tauri 2.x,
~42 MB idle, so it doesn't turn the Dell into a space heater.

> Scaffolded 2026-10-11. The Rust backend is written against the `windows`
> crate 0.58 API from documentation but has **not been compiled** — there is
> no Windows toolchain on the machine it was written on. See
> "First Windows build" below.

## Stack

- **Tauri 2.x** — desktop shell (WebView2 on Windows)
- **Rust** backend — WASAPI audio probing (`src-tauri/src/system_health.rs`)
- **React + TypeScript + Vite** frontend — the PASS/WARN dashboard
- Functional spec for the health checks: `../MajesticFox-HealthCheck-v01/MajesticFox-HealthCheck-v01.ps1`
- Architecture decision: `../majestic-fox-architecture.md`

## Prerequisites (on the Windows build machine)

1. **Rust stable** via [rustup](https://rustup.rs/) (`rustup default stable`)
2. **Node.js 20+** (LTS) + npm
3. **WebView2 Evergreen Runtime** — preinstalled on Windows 10 1803+ / 11 in
   practice; if missing, the installer bootstrapper fetches it
4. MSVC build tools (comes with "Desktop development with C++" in the
   Visual Studio installer — needed for Rust on Windows)

## Dev

```powershell
npm install
npm run tauri dev
```

Opens the app window (900×650) with hot reload.

## Build (Windows installer)

```powershell
npm run tauri build
```

Produces the NSIS installer + portable `.exe` under
`src-tauri/target/release/bundle/`. No auto-updater is wired up yet
(deliberate — Phase 1).

## Distribution (chosen: cloud builds)

Builds happen on **GitHub Actions**, not on the Dell — the Dell overheats
and is short on storage, so it never compiles anything. Pushing a tag like
`v0.1.0` triggers `.github/workflows/build.yml`, which builds the Windows
x86_64 app in the cloud and publishes a GitHub Release with the NSIS
`*-setup.exe` installer plus the portable `majestic-fox.exe` attached. Harry
downloads the installer from the Releases page — that's the whole install
story. Setup walkthrough (plain English): [docs/github-setup.md](docs/github-setup.md).

**Dell-local build: not recommended.** `npm run tauri build` on the Dell
works in principle (prerequisites are listed under "Prerequisites" above),
but a full Rust + Tauri compile pins the CPU for 10+ minutes on a machine
that already thermal-throttles, and the toolchain eats gigabytes Harry is
short on. Kept here for reference only.

## App icons

Placeholder icon set is checked in at `src-tauri/icons/` (amber geometric
fox on indigo, generated 2026-10-11). Replace with real branding any time:

```powershell
npm run tauri icon path\to\source-icon.png
```

## What Phase 1 covers

### Rig health
- `list_audio_endpoints` — enumerates every **active** playback + recording
  endpoint via the MMDevice API, reads each one's real mix-format sample rate
  via `IAudioClient::GetMixFormat`, reports friendly name, flow, rate,
  default-device flag. Per-endpoint failures degrade to `????` rows, never
  aborting the sweep.
- Dashboard renders PASS (48000 Hz) / WARN (anything else) / ???? (unreadable)
  with the exact Sound-settings fix path on WARN rows, plus an ALL CLEAR /
  action-needed banner and a Re-check button.
- COM is initialized per calling thread (Tauri commands run on a thread
  pool); all CoTaskMemAlloc'd buffers are freed.

### Profile library ("One script, many profiles")
- `list_profiles` / `get_profile` (`src-tauri/src/profiles.rs`) — pure file
  IO + serde, no Windows APIs. Reads the bundled seed file
  (`profiles/seed-profiles.json`, shipped via `bundle.resources`), with a dev
  fallback next to the crate; a user-profiles file in the app-data dir takes
  precedence when saving lands (later milestone).
- Seeds are **examples** (MW4 assault rifle, Warzone SMG) on the same 0-100
  scale as the Zen quick-nudge, so the numbers can be dialed in by hand today.
- Frontend `ProfileManager.tsx`: profile cards, detail view with values,
  active-profile state in localStorage, and a **disabled, clearly labeled**
  "Push to Zen — coming soon" button. Pushing values to the Zen automatically
  is a later milestone; the button must never look like it works when it
  doesn't.
- Tabs in `App.tsx`: Health | Profiles.

## What's stubbed / needs the first Windows build

- **`get_cpu_temp`** — stub. TODO: query the `root\LibreHardwareMonitor` WMI
  namespace (class `Sensor`, hottest CPU package/core temperature sensor),
  mirroring the v0.1 PowerShell script. Needs the `wmi` crate or raw
  IWbemLocator — decision deferred until it can be exercised against a real
  LibreHardwareMonitor install on the Dell.
- **Import paths to confirm** on first compile (flagged in
  `system_health.rs` module docs): `System::Variant::{PROPVARIANT,
  VT_LPWSTR}`, `System::Com::STGM_READ`, `PWSTR::to_string()`. The compiler
  will point at them directly; fallbacks are documented inline.
- **Distribution**: cloud builds via GitHub Actions — see the
  "Distribution (chosen: cloud builds)" section above and
  [docs/github-setup.md](docs/github-setup.md).

## Roadmap (later phases, not in this scaffold)

- Phase 1.x: user-profile saving (write path to the app-data `profiles.json`),
  Zen push link (automatic value push to the Zen over USB/serial)
- Phase 2: capture-card feed reading in-app (verify WebView2 `getUserMedia`
  against the real card; Rust `nokhwa` fallback), echo/monitoring config
  checks, chat co-pilot, **aim coaching** (input-side only — see
  `docs/aim-coaching-design.md`; the no-screen-reading hard line stands)
- Phase 3: studio tools (mastering presets, chopped-and-screwed), avatar
  builder, freemium packaging
