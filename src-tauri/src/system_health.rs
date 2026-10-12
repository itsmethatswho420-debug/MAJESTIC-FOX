//! System-health probes for Majestic Fox — Phase 1 (stream co-pilot).
//!
//! Functional spec: `MajesticFox-HealthCheck-v01.ps1` — enumerate every active
//! audio endpoint, read its mix-format sample rate via WASAPI `GetMixFormat`,
//! and flag anything that is not 48000 Hz (the sync-drift fix for the Dell).
//!
//! Platform note: the WASAPI implementation below is Windows-only and is
//! gated behind `#[cfg(target_os = "windows")]`. It was written against the
//! `windows` crate 0.58 API from documentation and fixed against the real
//! compiler on the first cloud builds (v0.1.1: import paths + HRESULT/UTF-16
//! conversions). Import paths verified:
//!   1. `windows::Win32::Media::Audio::WAVEFORMATEX`
//!   2. `windows::core::PROPVARIANT`
//!      (needs the `Win32_System_Com_StructuredStorage` crate feature)
//!   3. `windows::Win32::System::Variant::VT_LPWSTR`
//! If the friendly-name lookup ever breaks on a future `windows` version,
//! the fallback is the endpoint ID string (`IMMDevice::GetId`).

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
        core::{GUID, PROPVARIANT, PWSTR},
        Win32::{
            Foundation::{E_UNEXPECTED, RPC_E_CHANGED_MODE, S_FALSE, S_OK},
            Media::{
                Audio::{
                    IAudioClient, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator,
                    MMDeviceEnumerator, WAVEFORMATEX,
                    DEVICE_STATE_ACTIVE, eCapture, eMultimedia, eRender,
                },
            },
            System::{
                Com::{
                    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                    CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
                },
                Variant::VT_LPWSTR,
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

    // Fallback property keys, tried in order when the friendly name is
    // missing: PKEY_Device_DeviceDesc (same fmtid, pid 2) and
    // PKEY_DeviceInterface_FriendlyName
    // ({026E516E-B814-414B-83CD-856D425EFCFE}, pid 2).
    const PKEY_DEVICE_DEVICE_DESC: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0xA45C254E_DF1C_4EFD_8020_67D146A8F0E),
        pid: 2,
    };
    const PKEY_DEVICEINTERFACE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x026E516E_B814_414B_83CD_856D425EFCFE),
        pid: 2,
    };

    // STGM_READ comes from windows::Win32::System::Com (imported above).

    /// RAII guard: Tauri commands run on a thread-pool, and COM requires
    /// per-thread initialization. MTA is fine for the MMDevice APIs.
    struct ComGuard {
        initialized: bool,
    }
    impl ComGuard {
        unsafe fn init() -> windows::core::Result<Self> {
            // Tauri/WebView2 threads often already run COM in STA mode. Asking
            // for MTA then returns RPC_E_CHANGED_MODE ("cannot change thread
            // mode after it is set") — that is NOT fatal: COM is alive on this
            // thread, and WASAPI works fine from either apartment. Only fail
            // on a genuinely bad HRESULT. We track whether WE initialized so
            // Drop only uninitializes what we own.
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr == S_OK || hr == S_FALSE {
                Ok(Self { initialized: true })
            } else if hr == RPC_E_CHANGED_MODE {
                Ok(Self { initialized: false })
            } else {
                Err(windows::core::Error::from(hr))
            }
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.initialized {
                unsafe { CoUninitialize() };
            }
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
                // NOTE: PWSTR::to_string() yields Result<_, FromUtf16Error>,
                // so the closure must wrap it in Ok(..) to satisfy and_then's
                // Result<_, windows::core::Error> requirement.
                let default_id: Option<String> = enumerator
                    .GetDefaultAudioEndpoint(flow, eMultimedia)
                    .and_then(|d| d.GetId())
                    .and_then(|pwsz| Ok(pwsz.to_string()?))
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
                .and_then(|pwsz| Ok(pwsz.to_string()?))
                .unwrap_or_default();
            ep.is_default = default_id.map(|d| d == id).unwrap_or(false);
            // Friendly name when we can get it; otherwise the raw endpoint ID.
            // Primary: IPropertyStore. Fallback: the same registry value the
            // v0.1 PowerShell script read (proven on Harry's Dell).
            ep.name = friendly_name(&device)
                .ok()
                .filter(|n| !n.is_empty())
                .or_else(|| friendly_name_registry(&id, flow_name))
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

    /// Registry fallback for the friendly name, mirroring the v0.1 PowerShell
    /// script (HKLM\...\MMDevices\Audio\{Render|Capture}\{guid}\Properties).
    /// Endpoint IDs look like "{0.0.0.00000000}.{guid}" where the guid part
    /// already carries braces (occasionally parens on odd drivers). Strip
    /// either wrapper, then look up the canonical "{guid}" key name.
    fn friendly_name_registry(device_id: &str, flow_name: &str) -> Option<String> {
        let segment = device_id.rsplit('.').next()?.trim();
        // Strip one layer of braces or parens, whichever wraps the guid.
        let clean = segment
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .or_else(|| {
                segment
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
            })
            .unwrap_or(segment)
            .trim();
        if clean.is_empty() {
            return None;
        }
        let flow_key = if flow_name == "playback" {
            "Render"
        } else {
            "Capture"
        };
        let base = format!(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\{flow_key}"
        );
        let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
        // Canonical "{guid}" first, then the bare guid as a last resort.
        for key_name in [format!("{{{clean}}}"), clean.to_string()] {
            let path = format!("{base}\\{key_name}\\Properties");
            if let Ok(key) = hklm.open_subkey(&path) {
                if let Ok(name) = key.get_value::<String, _>(
                    "{a45c254e-df1c-4efd-8020-67d146a8f0e},14",
                ) {
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    /// Read a display name from the endpoint's property store. Tries, in
    /// order: PKEY_Device_FriendlyName, PKEY_Device_DeviceDesc,
    /// PKEY_DeviceInterface_FriendlyName. Returns Err only when all three
    /// are missing — the caller then falls back to the registry lookup.
    fn friendly_name(device: &IMMDevice) -> windows::core::Result<String> {
        unsafe {
            let store: IPropertyStore = device.OpenPropertyStore(STGM_READ)?;
            for key in [
                &PKEY_DEVICE_FRIENDLY_NAME,
                &PKEY_DEVICE_DEVICE_DESC,
                &PKEY_DEVICEINTERFACE_FRIENDLY_NAME,
            ] {
                if let Ok(name) = read_prop_string(&store, key) {
                    if !name.is_empty() {
                        return Ok(name);
                    }
                }
            }
            Err(windows::core::Error::new(
                E_UNEXPECTED,
                "no usable device name property found",
            ))
        }
    }

    /// Read one LPWSTR property from an IPropertyStore. Ownership of the
    /// CoTaskMemAlloc'd string transfers to us — convert, then free.
    fn read_prop_string(
        store: &IPropertyStore,
        key: &PROPERTYKEY,
    ) -> windows::core::Result<String> {
        unsafe {
            // PROPVARIANT is a newtype wrapper in windows-core 0.58, so field
            // access goes through as_raw(). Raw layout:
            // imp::PROPVARIANT { Anonymous: PROPVARIANT_0 (union)
            //   { Anonymous: PROPVARIANT_0_0 { vt: VARENUM, ...,
            //     Anonymous: PROPVARIANT_0_0_0 (union with pwszVal) } } }.
            let pv: PROPVARIANT = store.GetValue(key)?;
            if pv.as_raw().Anonymous.Anonymous.vt != VT_LPWSTR.0 {
                return Err(windows::core::Error::new(
                    E_UNEXPECTED,
                    "device name property is not a string",
                ));
            }
            let pwsz: PWSTR = PWSTR(pv.as_raw().Anonymous.Anonymous.Anonymous.pwszVal);
            let name = pwsz.to_string().map_err(|e| {
                windows::core::Error::new(
                    E_UNEXPECTED,
                    format!("device name is not valid UTF-16: {e:?}"),
                )
            });
            CoTaskMemFree(Some(pwsz.0 as *const core::ffi::c_void));
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
            CoTaskMemFree(Some(fmt as *const core::ffi::c_void));
            Ok(hz)
        }
    }
}
