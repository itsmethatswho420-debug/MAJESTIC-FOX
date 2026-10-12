import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Every device on the rig must run at this rate — the sync-drift fix. */
const TARGET_HZ = 48_000;

/** Mirrors `AudioEndpoint` in src-tauri/src/system_health.rs (camelCase serde). */
interface AudioEndpoint {
  name: string;
  flow: string; // "playback" | "recording"
  sampleRateHz: number;
  isDefault: boolean;
  error: string | null;
}

/** Mirrors `CpuTemp` in src-tauri/src/system_health.rs. */
interface CpuTemp {
  celsius: number | null;
  available: boolean;
  note: string;
}

type EndpointStatus = "pass" | "warn" | "unknown";

function statusOf(ep: AudioEndpoint): EndpointStatus {
  if (ep.error || ep.sampleRateHz === 0) return "unknown";
  return ep.sampleRateHz === TARGET_HZ ? "pass" : "warn";
}

const STATUS_LABEL: Record<EndpointStatus, string> = {
  pass: "PASS",
  warn: "WARN",
  unknown: "????",
};

function formatHz(hz: number): string {
  if (hz === 0) return "unreadable";
  return `${hz.toLocaleString()} Hz`;
}

export default function HealthDashboard() {
  const [endpoints, setEndpoints] = useState<AudioEndpoint[] | null>(null);
  const [cpu, setCpu] = useState<CpuTemp | null>(null);
  const [loading, setLoading] = useState(false);
  const [fatal, setFatal] = useState<string | null>(null);

  const runCheck = useCallback(async () => {
    setLoading(true);
    setFatal(null);
    try {
      const [eps, temp] = await Promise.all([
        invoke<AudioEndpoint[]>("list_audio_endpoints"),
        invoke<CpuTemp>("get_cpu_temp"),
      ]);
      setEndpoints(eps);
      setCpu(temp);
    } catch (e) {
      // invoke() rejects outside the Tauri webview (plain browser) or when
      // the Rust command returns Err — either way, say so plainly.
      setFatal(
        e instanceof Error
          ? e.message
          : typeof e === "string"
            ? e
            : "Could not reach the system-health backend. Are you running inside the Tauri app?",
      );
      setEndpoints(null);
      setCpu(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void runCheck();
  }, [runCheck]);

  const counts = (endpoints ?? []).reduce(
    (acc, ep) => {
      acc[statusOf(ep)] += 1;
      return acc;
    },
    { pass: 0, warn: 0, unknown: 0 } as Record<EndpointStatus, number>,
  );
  const allClear =
    endpoints !== null && counts.warn === 0 && counts.unknown === 0 && endpoints.length > 0;

  return (
    <section className="card">
      <div className="card-head">
        <h2>Audio devices &mdash; want {TARGET_HZ.toLocaleString()} Hz everywhere</h2>
        <button className="btn" onClick={() => void runCheck()} disabled={loading}>
          {loading ? "Checking…" : "Re-check"}
        </button>
      </div>

      {fatal && <div className="banner banner-error">{fatal}</div>}

      {!fatal && allClear && (
        <div className="banner banner-ok">
          ALL CLEAR — every readable device is at {TARGET_HZ.toLocaleString()} Hz. Good to go live.
        </div>
      )}

      {!fatal && endpoints !== null && counts.warn > 0 && (
        <div className="banner banner-warn">
          {counts.warn} device{counts.warn === 1 ? " is" : "s are"} NOT at{" "}
          {TARGET_HZ.toLocaleString()} Hz. Fix: Sound settings &rarr; device Properties &rarr;
          Advanced &rarr; Default Format &rarr; {TARGET_HZ.toLocaleString()} Hz, then re-check.
        </div>
      )}

      <ul className="endpoint-list">
        {(endpoints ?? []).map((ep, i) => {
          const status = statusOf(ep);
          return (
            <li key={`${ep.flow}-${ep.name}-${i}`} className={`endpoint endpoint-${status}`}>
              <span className={`pill pill-${status}`}>{STATUS_LABEL[status]}</span>
              <div className="endpoint-body">
                <div className="endpoint-name">
                  {ep.name || "(unnamed device)"}
                  {ep.isDefault && <span className="default-tag">default</span>}
                </div>
                <div className="endpoint-meta">
                  {ep.flow} &middot; {formatHz(ep.sampleRateHz)}
                  {status === "warn" && (
                    <span className="fix-hint">
                      {" "}
                      &mdash; set to {TARGET_HZ.toLocaleString()} Hz in Sound settings
                    </span>
                  )}
                  {ep.error && <span className="fix-hint"> &mdash; {ep.error}</span>}
                </div>
              </div>
            </li>
          );
        })}
      </ul>

      {!fatal && endpoints !== null && endpoints.length === 0 && (
        <p className="muted">No active audio endpoints found.</p>
      )}

      {!fatal && endpoints === null && (
        <p className="muted">Probing audio devices…</p>
      )}

      <div className="temp-block">
        <h3>CPU temperature</h3>
        {cpu === null && !fatal && <p className="muted">Reading…</p>}
        {cpu !== null && cpu.available && cpu.celsius !== null && (
          <p
            className={
              cpu.celsius >= 90 ? "temp-hot" : cpu.celsius >= 80 ? "temp-warm" : "temp-ok"
            }
          >
            {cpu.celsius >= 90
              ? `WARN — CPU ~${cpu.celsius.toFixed(1)} °C: hot, expect throttling / drift.`
              : cpu.celsius >= 80
                ? `CPU ~${cpu.celsius.toFixed(1)} °C: warm, keep an eye on it.`
                : `PASS — CPU ~${cpu.celsius.toFixed(1)} °C.`}
          </p>
        )}
        {cpu !== null && !cpu.available && <p className="muted">{cpu.note}</p>}
      </div>
    </section>
  );
}
