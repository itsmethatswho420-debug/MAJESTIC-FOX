//! Rig Doctor — weak-hardware diagnostics for Majestic Fox.
//!
//! Harry's rule: nobody gets told "buy a better PC." This module reads the
//! machine the app is actually running on — CPU load, memory pressure, disk
//! space, network throughput, temperatures — and reports what's choking the
//! stream so the fix is tuning, not shopping.
//!
//! Platform: Windows-only (sysinfo works cross-platform, but the WMI
//! temperature path and the app's packaging target Windows). The `sysinfo`
//! crate handles CPU/memory/disk/network; CPU temperature goes through
//! LibreHardwareMonitor's WMI namespace when it's installed (free).

use serde::Serialize;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind, System};
use tauri::command;

/// One disk's space picture.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total_gb: f64,
    pub available_gb: f64,
    /// 0–100.
    pub used_pct: f64,
}

/// One network interface's recent throughput.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetIface {
    pub name: String,
    pub received_mb: f64,
    pub transmitted_mb: f64,
}

/// Everything Rig Doctor measures in one sweep.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    /// Overall CPU load 0–100.
    pub cpu_pct: f32,
    /// Per-core load 0–100.
    pub cores_pct: Vec<f32>,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub mem_total_gb: f64,
    pub mem_used_gb: f64,
    /// 0–100.
    pub mem_pct: f64,
    pub disks: Vec<DiskInfo>,
    pub net: Vec<NetIface>,
    /// Best-effort CPU temperature, Celsius. None when no sensor is readable.
    pub cpu_temp_c: Option<f32>,
    pub temp_source: String,
}

/// Full Rig Doctor sweep: CPU, memory, disks, network, temperature.
#[command]
pub fn get_system_stats() -> Result<SystemStats, String> {
    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    // CPU usage needs two samples ~1s apart to be meaningful.
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu_pct = sys.global_cpu_usage();
    let cores_pct: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "unknown CPU".to_string());

    let mem_total = sys.total_memory() as f64 / 1_073_741_824.0;
    let mem_used = sys.used_memory() as f64 / 1_073_741_824.0;
    let mem_pct = if mem_total > 0.0 {
        mem_used / mem_total * 100.0
    } else {
        0.0
    };

    let disks = Disks::new_with_refreshed_list();
    let disk_info: Vec<DiskInfo> = disks
        .iter()
        .map(|d| {
            let total = d.total_space() as f64 / 1_073_741_824.0;
            let avail = d.available_space() as f64 / 1_073_741_824.0;
            let used_pct = if total > 0.0 {
                (total - avail) / total * 100.0
            } else {
                0.0
            };
            DiskInfo {
                name: d.name().to_string_lossy().to_string(),
                mount: d.mount_point().to_string_lossy().to_string(),
                total_gb: (total * 10.0).round() / 10.0,
                available_gb: (avail * 10.0).round() / 10.0,
                used_pct: (used_pct * 10.0).round() / 10.0,
            }
        })
        .collect();

    let networks = Networks::new_with_refreshed_list();
    let net: Vec<NetIface> = networks
        .iter()
        .filter(|(_, n)| n.received() > 0 || n.transmitted() > 0)
        .map(|(name, n)| NetIface {
            name: name.clone(),
            received_mb: (n.received() as f64 / 1_048_576.0 * 10.0).round() / 10.0,
            transmitted_mb: (n.transmitted() as f64 / 1_048_576.0 * 10.0).round() / 10.0,
        })
        .collect();

    let (cpu_temp_c, temp_source) = read_cpu_temp();

    Ok(SystemStats {
        cpu_pct: (cpu_pct * 10.0).round() / 10.0,
        cores_pct: cores_pct
            .into_iter()
            .map(|v| (v * 10.0).round() / 10.0)
            .collect(),
        cpu_name,
        cpu_cores: sys.cpus().len(),
        mem_total_gb: (mem_total * 10.0).round() / 10.0,
        mem_used_gb: (mem_used * 10.0).round() / 10.0,
        mem_pct: (mem_pct * 10.0).round() / 10.0,
        disks: disk_info,
        net,
        cpu_temp_c,
        temp_source,
    })
}

/// Best-effort CPU temperature.
///
/// Tries, in order:
/// 1. LibreHardwareMonitor's WMI namespace (`root\LibreHardwareMonitor`,
///    class `Sensor`, SensorType "Temperature", CPU package sensor) —
///    free, accurate, the recommended install.
/// 2. Nothing else on Windows exposes this reliably without a driver, so
///    when LHM isn't there we say so plainly instead of inventing a number.
#[cfg(target_os = "windows")]
fn read_cpu_temp() -> (Option<f32>, String) {
    use std::process::Command;
    // PowerShell one-liner against the LHM WMI namespace. Short timeout;
    // failure just means "not installed".
    let ps = r#"Get-CimInstance -Namespace root/LibreHardwareMonitor -ClassName Sensor -ErrorAction SilentlyContinue | Where-Object { $_.SensorType -eq 'Temperature' -and $_.Name -like '*CPU Package*' } | Select-Object -First 1 -ExpandProperty Value"#;
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            match s.parse::<f32>() {
                Ok(v) if v > 0.0 && v < 150.0 => {
                    (Some((v * 10.0).round() / 10.0), "LibreHardwareMonitor".to_string())
                }
                _ => (
                    None,
                    "LibreHardwareMonitor found but no CPU package sensor readable.".to_string(),
                ),
            }
        }
        _ => (
            None,
            "Install LibreHardwareMonitor (free) for CPU temperature readings.".to_string(),
        ),
    }
}

#[cfg(not(target_os = "windows"))]
fn read_cpu_temp() -> (Option<f32>, String) {
    (
        None,
        "CPU temperature is implemented for Windows only.".to_string(),
    )
}

/// Plain-language diagnosis: what's choking this rig, and what to do about
/// it. No "buy a better PC" — only tuning the machine in front of us.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    pub level: String, // "ok" | "watch" | "fix"
    pub headline: String,
    pub detail: String,
    pub fixes: Vec<String>,
}

#[command]
pub fn diagnose_rig() -> Result<Vec<Diagnosis>, String> {
    let s = get_system_stats()?;
    let mut out: Vec<Diagnosis> = Vec::new();

    // CPU pressure
    if s.cpu_pct >= 90.0 {
        out.push(Diagnosis {
            level: "fix".to_string(),
            headline: "CPU is maxed out".to_string(),
            detail: format!(
                "{} at {:.1}% — the stream is fighting the machine for cycles.",
                s.cpu_name, s.cpu_pct
            ),
            fixes: vec![
                "Close browsers and launchers before going live (Chrome eats cores).".to_string(),
                "In OBS/TikTok Studio: step the encoder preset down one notch.".to_string(),
                "Kill background updaters: Task Manager → Startup → disable what you don't need.".to_string(),
            ],
        });
    } else if s.cpu_pct >= 75.0 {
        out.push(Diagnosis {
            level: "watch".to_string(),
            headline: "CPU is working hard".to_string(),
            detail: format!("{:.1}% load — fine now, but one more app could tip it.", s.cpu_pct),
            fixes: vec![
                "Keep it lean while live: game + stream software, nothing else.".to_string(),
            ],
        });
    }

    // Memory pressure
    if s.mem_pct >= 90.0 {
        out.push(Diagnosis {
            level: "fix".to_string(),
            headline: "Memory is nearly full".to_string(),
            detail: format!(
                "{:.1} of {:.1} GB used ({:.0}%) — Windows is paging, everything stutters.",
                s.mem_used_gb, s.mem_total_gb, s.mem_pct
            ),
            fixes: vec![
                "Close the browser while streaming — it's the usual hog.".to_string(),
                "Restart the machine before a long stream to clear the creep.".to_string(),
            ],
        });
    } else if s.mem_pct >= 80.0 {
        out.push(Diagnosis {
            level: "watch".to_string(),
            headline: "Memory is getting tight".to_string(),
            detail: format!("{:.0}% used — watch it on long sessions.", s.mem_pct),
            fixes: vec!["Close what you're not using before going live.".to_string()],
        });
    }

    // Disk space (stream drive)
    for d in &s.disks {
        if d.used_pct >= 95.0 {
            out.push(Diagnosis {
                level: "fix".to_string(),
                headline: format!("Disk {} is almost full", d.mount),
                detail: format!(
                    "{:.1} GB free of {:.1} GB — recordings can fail mid-stream.",
                    d.available_gb, d.total_gb
                ),
                fixes: vec![
                    "Move finished recordings off this drive (TeraBox works).".to_string(),
                    "Empty the recycle bin — it counts against you.".to_string(),
                ],
            });
        } else if d.used_pct >= 85.0 {
            out.push(Diagnosis {
                level: "watch".to_string(),
                headline: format!("Disk {} is filling up", d.mount),
                detail: format!("{:.0}% used.", d.used_pct),
                fixes: vec!["Plan a cleanup before the next long recording.".to_string()],
            });
        }
    }

    // Temperature
    if let Some(t) = s.cpu_temp_c {
        if t >= 90.0 {
            out.push(Diagnosis {
                level: "fix".to_string(),
                headline: "CPU is overheating".to_string(),
                detail: format!(
                    "{:.1} °C — the chip is throttling itself, frames will drop.",
                    t
                ),
                fixes: vec![
                    "Point the fan at the vents and give it 5 minutes.".to_string(),
                    "Check the vents for dust — the Dell's been running hot.".to_string(),
                    "Take a break; streaming through throttle just looks bad.".to_string(),
                ],
            });
        } else if t >= 80.0 {
            out.push(Diagnosis {
                level: "watch".to_string(),
                headline: "CPU is running warm".to_string(),
                detail: format!("{:.1} °C — okay, but trending toward throttle.", t),
                fixes: vec!["Keep the airflow going; check again in 30 minutes.".to_string()],
            });
        }
    }

    if out.is_empty() {
        out.push(Diagnosis {
            level: "ok".to_string(),
            headline: "Rig looks healthy".to_string(),
            detail: "Nothing choking the stream right now. Go live.".to_string(),
            fixes: vec![],
        });
    }

    Ok(out)
}
