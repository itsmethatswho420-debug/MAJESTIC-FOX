//! System-health probes for Majestic Fox — Phase 1 (stream co-pilot).
//!
//! Functional spec: `MajesticFox-HealthCheck-v01.ps1` — enumerate every active
//! audio endpoint, read its mix-format sample rate via WASAPI `GetMixFormat`,
//! and flag anything that is not 48000 Hz (the sync-drift fix for the Dell).
//!
//! Platform note: the WASAPI implementation below is Windows-only and is
//! gated behind `#[cfg(target_os = "windows")]`. It has been written against
//! the `windows` crate 0.58 API surface from documentation, but it has NOT
//! been compiled (no Windows toolchain on this machine). On the first real
//! Windows build, confirm these import paths in particular — the compiler
//! will flag them immediately if they are wrong:
//!   1. `windows::Win32::System::Variant::{PROPVARIANT, VT_LPWSTR}`
//!   2. `windows::Win32::System::Com::STGM_READ` (STGM newtype constant)
//!   3. `PWSTR::to_string()` availability on the pinned `windows` version.
//! If (1) is wrong, the fallback is to use the endpoint ID string
//! (`IMMDevice::GetId`) as the display name and drop the friendly-name lookup.

use serde::Serialize;
use tauri::command;

/// The sample rate every device on the rig must run at. Mismatches here are
/// the prime suspect behind the Dell's audio drifting out of sync on long
/// streams (and behind the 48 kHz guidance in the setup walkthroughs).
pub const TARGET_SAMPLE_RATE_HZ: u32 = 48_000;

/// One audio endpoint as seen by Windows' MMDevice API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioEndpoint {
    /// Friendly name, e.g. "PlayStation Headset". Falls back to the endpoint
    /// ID string when the name cannot be read.
    pub name: String,
    /// "playback" (render) or "recording" (capture).
    pub flow: String,
    /// Mix-format sample rate in Hz. 0 = unreadable.
    pub sample_rate_hz: u32,
    /// True when this is the default endpoint for its flow / role.
    pub is_default: bool,
    /// Set when the endpoint could be opened but not fully probed.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuTemp {
    pub celsius: Option<f32>,
    pub available: bool,
    pub note: String,
}

/// List every active audio endpoint with its real mix-format sample rate.
#[command]
pub fn list_audio_endpoints() -> Result<Vec<AudioEndpoint>, String> {
    #[cfg(target_os = "windows")]
    {
        windows_impl::list_audio_endpoints().map_err(|e| format!("WASAPI probe failed: {e:?}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Audio endpoint probing is implemented for Windows only.".to_string())
    }
}

/// CPU temperature (best effort — needs LibreHardwareMonitor's WMI sensors).
#[command]
pub fn get_cpu_temp() -> Result<CpuTemp, String> {
    // TODO: query the `root\LibreHardwareMonitor` WMI namespace
    // (class `Sensor`, SensorType == "Temperature", hottest CPU package/core
    // sensor), mirroring MajesticFox-HealthCheck-v01.ps1. Needs either the
    // `wmi` crate or raw IWbemLocator via windows-rs. Stubbed until the crate
    // choice is made and it can be exercised against a real install on the
    // Dell — do NOT invent a temperature value here.
    Ok(CpuTemp {
        celsius: None,
        available: false,
        note: "CPU temperature probe is not wired up yet. Install LibreHardwareMonitor (free); this check will read its WMI sensors once implemented.".to_string(),
    })
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::AudioEndpoint;
    use windows::{
        core::{GUID, PWSTR},
        Win32::{
            Foundation::E_UNEXPECTED,
            Media::{
                Audio::{
                    IAudioClient, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator,
                    MMDeviceEnumerator, DEVICE_STATE_ACTIVE, eCapture, eMultimedia, eRender,
                },
                WAVEFORMATEX,
            },
            System::{
                Com::{
                    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                    CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
                },
                Variant::{PROPVARIANT, VT_LPWSTR},
            },
            UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY},
        },
    };

    // PKEY_Device_FriendlyName = {A45C254E-DF1C-4EFD-8020-67D146A8F0E}, pid 14.
    // Defined manually so we don't depend on the PKEY_* constant module path.
    const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0xA45C254E_DF1C_4EFD_8020_67D146A8F0E),
        pid: 14,
    };

    // STGM_READ comes from windows::Win32::System::Com (imported above).

    /// RAII guard: Tauri commands run on a thread-pool, and COM requires
    /// per-thread initialization. MTA is fine for the MMDevice APIs.
    struct ComGuard;
    impl ComGuard {
        unsafe fn init() -> windows::core::Result<Self> {
            // S_OK or S_FALSE (already initialized) both map to Ok here;
            // a real failure (e.g. conflicting apartment mode) surfaces.
            CoInitializeEx(None, COINIT_MULTITHREADED)?;
            Ok(Self)
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    pub fn list_audio_endpoints() -> windows::core::Result<Vec<AudioEndpoint>> {
        unsafe {
            let _com = ComGuard::init()?;

            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            let mut out = Vec::new();
            for (flow, flow_name) in [(eRender, "playback"), (eCapture, "recording")] {
                let collection: IMMDeviceCollection =
                    enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;

                // Default endpoint for this flow (best effort — may be absent).
                let default_id: Option<String> = enumerator
                    .GetDefaultAudioEndpoint(flow, eMultimedia)
                    .and_then(|d| d.GetId())
                    .and_then(|pwsz| pwsz.to_string())
                    .ok();

                let count = collection.GetCount()?;
                for i in 0..count {
                    out.push(probe_endpoint(&collection, i, flow_name, default_id.as_deref()));
                }
            }
            Ok(out)
        }
    }

    /// Probe one endpoint. Never panics and never aborts the whole sweep:
    /// per-endpoint failures are recorded on the endpoint itself, mirroring
    /// the v0.1 PowerShell script's `??` rows.
    fn probe_endpoint(
        collection: &IMMDeviceCollection,
        index: u32,
        flow_name: &str,
        default_id: Option<&str>,
    ) -> AudioEndpoint {
        unsafe {
            let mut ep = AudioEndpoint {
                name: String::new(),
                flow: flow_name.to_string(),
                sample_rate_hz: 0,
                is_default: false,
                error: None,
            };

            let device: IMMDevice = match collection.Item(index) {
                Ok(d) => d,
                Err(e) => {
                    ep.error = Some(format!("could not open endpoint: {e:?}"));
                    return ep;
                }
            };

            let id = device
                .GetId()
                .and_then(|pwsz| pwsz.to_string())
                .unwrap_or_default();
            ep.is_default = default_id.map(|d| d == id).unwrap_or(false);
            // Friendly name when we can get it; otherwise the raw endpoint ID.
            ep.name = friendly_name(&device)
                .ok()
                .filter(|n| !n.is_empty())
                .unwrap_or(id);

            match mix_sample_rate(&device) {
                Ok(hz) => ep.sample_rate_hz = hz,
                Err(e) => {
                    ep.error = Some(format!("could not read mix format: {e:?}"));
                }
            }
            ep
        }
    }

    /// Read PKEY_Device_FriendlyName from the endpoint's property store.
    fn friendly_name(device: &IMMDevice) -> windows::core::Result<String> {
        unsafe {
            // NOTE: verify `PROPVARIANT` / `VT_LPWSTR` import path on first
            // Windows build (see module docs). The union access below follows
            // the windows-rs generated layout: PROPVARIANT { Anonymous:
            // PROPVARIANT_0 { vt, ..., Anonymous: PROPVARIANT_0_0 (union) } }.
            let store: IPropertyStore = device.OpenPropertyStore(STGM_READ)?;
            let pv: PROPVARIANT = store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME)?;
            if pv.Anonymous.vt != VT_LPWSTR {
                return Err(windows::core::Error::new(
                    E_UNEXPECTED,
                    "friendly name property is not a string",
                ));
            }
            let pwsz: PWSTR = pv.Anonymous.Anonymous.pwszVal;
            let name = pwsz.to_string();
            // GetValue hands us a CoTaskMemAlloc'd LPWSTR — we own it now.
            // (Avoids depending on the PropVariantClear import path.)
            CoTaskMemFree(pwsz.0 as *const core::ffi::c_void);
            name
        }
    }

    /// The heart of the health check: the endpoint's real mix-format sample
    /// rate straight from WASAPI — the same value the v0.1 script reads.
    fn mix_sample_rate(device: &IMMDevice) -> windows::core::Result<u32> {
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let fmt: *mut WAVEFORMATEX = client.GetMixFormat()?;
            if fmt.is_null() {
                return Err(windows::core::Error::new(
                    E_UNEXPECTED,
                    "GetMixFormat returned a null format pointer",
                ));
            }
            // WAVEFORMATEXTENSIBLE shares the WAVEFORMATEX prefix, so
            // nSamplesPerSec is valid either way.
            let hz = (*fmt).nSamplesPerSec;
            CoTaskMemFree(fmt as *const core::ffi::c_void);
            Ok(hz)
        }
    }
}
