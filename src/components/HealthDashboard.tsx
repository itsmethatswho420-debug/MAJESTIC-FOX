import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Every device on the rig must run at this rate — the sync-drift fix. */
const TARGET_HZ = 48_000;

/** Mirrors `AudioEndpoint` in src-tauri/src/system_health.rs (camelCase serde). */
interface AudioEndpoint {
  name: string;
  /** Diagnostic: which lookup produced the name ("property" | "registry" | "registry-enum" | "raw"). */
  nameSource: string;
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

/** Mirrors `ZenPresence` in src-tauri/src/system_health.rs. */
interface ZenPresence {
  connected: boolean;
  detail: string;
}

/** Mirrors `SystemStats` in src-tauri/src/rig_doctor.rs (camelCase serde). */
interface SystemStats {
  cpuPct: number;
  coresPct: number[];
  cpuName: string;
  cpuCores: number;
  memTotalGb: number;
  memUsedGb: number;
  memPct: number;
  disks: { name: string; mount: string; totalGb: number; availableGb: number; usedPct: number }[];
  net: { name: string; receivedMb: number; transmittedMb: number }[];
  cpuTempC: number | null;
  tempSource: string;
}

/** Mirrors `Diagnosis` in src-tauri/src/rig_doctor.rs. */
interface Diagnosis {
  level: "ok" | "watch" | "fix";
  headline: string;
  detail: string;
  fixes: string[];
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

interface ReadinessCheck {
  label: string;
  detail: string;
  points: number;
  max: number;
}

/**
 * Go-live readiness score (0–100), computed only from what this tab actually
 * measures — no invented inputs. Weights: sample-rate health 50, device
 * readability 20, CPU temperature 20, playback+capture coverage 10.
 */
function readiness(endpoints: AudioEndpoint[] | null, cpu: CpuTemp | null): {
  score: number;
  checks: ReadinessCheck[];
} {
  const checks: ReadinessCheck[] = [];
  if (endpoints === null || endpoints.length === 0) {
    return { score: 0, checks };
  }

  const readable = endpoints.filter((e) => !e.error && e.sampleRateHz !== 0);
  const onTarget = readable.filter((e) => e.sampleRateHz === TARGET_HZ);
  const ratePoints = Math.round((50 * onTarget.length) / endpoints.length);
  checks.push({
    label: "Sample rates at 48,000 Hz",
    detail:
      onTarget.length === endpoints.length
        ? "every device is on target"
        : `${endpoints.length - onTarget.length} of ${endpoints.length} need fixing`,
    points: ratePoints,
    max: 50,
  });

  const unknown = endpoints.length - readable.length;
  const readPoints = unknown === 0 ? 20 : Math.max(0, 20 - unknown * 10);
  checks.push({
    label: "Devices readable",
    detail: unknown === 0 ? "all devices probed cleanly" : `${unknown} unreadable`,
    points: readPoints,
    max: 20,
  });

  let tempPoints = 10;
  let tempDetail = "no temperature reading — not penalized";
  if (cpu !== null && cpu.available && cpu.celsius !== null) {
    if (cpu.celsius < 80) {
      tempPoints = 20;
      tempDetail = `CPU ~${cpu.celsius.toFixed(1)} °C — cool`;
    } else if (cpu.celsius < 90) {
      tempPoints = 10;
      tempDetail = `CPU ~${cpu.celsius.toFixed(1)} °C — warm, watch it`;
    } else {
      tempPoints = 0;
      tempDetail = `CPU ~${cpu.celsius.toFixed(1)} °C — hot, expect drift`;
    }
  }
  checks.push({ label: "CPU temperature", detail: tempDetail, points: tempPoints, max: 20 });

  const hasPlayback = endpoints.some((e) => e.flow === "playback");
  const hasCapture = endpoints.some((e) => e.flow === "recording");
  const coverPoints = hasPlayback && hasCapture ? 10 : 0;
  checks.push({
    label: "Playback + capture present",
    detail:
      hasPlayback && hasCapture
        ? "both directions covered"
        : `missing ${[!hasPlayback && "playback", !hasCapture && "capture"].filter(Boolean).join(" + ")}`,
    points: coverPoints,
    max: 10,
  });

  return { score: checks.reduce((s, c) => s + c.points, 0), checks };
}

function verdictOf(score: number): { label: string; cls: string } {
  if (score >= 90) return { label: "READY — go live", cls: "banner-ok" };
  if (score >= 70) return { label: "ALMOST — fix the flagged items", cls: "banner-warn" };
  return { label: "NOT YET — fix before going live", cls: "banner-error" };
}

export default function HealthDashboard() {
  const [endpoints, setEndpoints] = useState<AudioEndpoint[] | null>(null);
  const [cpu, setCpu] = useState<CpuTemp | null>(null);
  const [stats, setStats] = useState<SystemStats | null>(null);
  const [diagnosis, setDiagnosis] = useState<Diagnosis[] | null>(null);
  const [zen, setZen] = useState<ZenPresence | null>(null);
  const [loading, setLoading] = useState(false);
  const [fatal, setFatal] = useState<string | null>(null);

  const runCheck = useCallback(async () => {
    setLoading(true);
    setFatal(null);
    try {
      const [eps, temp, sys, diag, zp] = await Promise.all([
        invoke<AudioEndpoint[]>("list_audio_endpoints"),
        invoke<CpuTemp>("get_cpu_temp"),
        invoke<SystemStats>("get_system_stats"),
        invoke<Diagnosis[]>("diagnose_rig"),
        invoke<ZenPresence>("get_zen_presence"),
      ]);
      setEndpoints(eps);
      setCpu(temp);
      setStats(sys);
      setDiagnosis(diag);
      setZen(zp);
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
      setStats(null);
      setDiagnosis(null);
      setZen(null);
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

  const ready = readiness(endpoints, cpu);
  const verdict = verdictOf(ready.score);

  return (
    <section className="card">
      <div className="card-head">
        <h2>Audio devices &mdash; want {TARGET_HZ.toLocaleString()} Hz everywhere</h2>
        <button className="btn" onClick={() => void runCheck()} disabled={loading}>
          {loading ? "Checking…" : "Re-check"}
        </button>
      </div>

      {fatal && <div className="banner banner-error">{fatal}</div>}

      {!fatal && zen !== null && (
        <div className={`banner ${zen.connected ? "banner-ok" : "banner-warn"}`}>
          {zen.connected ? "Zen connected" : "Zen not detected"} — {zen.detail}
        </div>
      )}

      {!fatal && endpoints !== null && endpoints.length > 0 && (
        <div className="readiness">
          <div className="readiness-top">
            <div className="readiness-score">
              <span className="readiness-number">{ready.score}</span>
              <span className="readiness-of">/100</span>
            </div>
            <div className={`banner ${verdict.cls} readiness-verdict`}>{verdict.label}</div>
          </div>
          <ul className="readiness-checks">
            {ready.checks.map((c) => (
              <li key={c.label} className="readiness-check">
                <span className={`pill ${c.points === c.max ? "pill-pass" : c.points === 0 ? "pill-error" : "pill-warn"}`}>
                  {c.points}/{c.max}
                </span>
                <div className="endpoint-body">
                  <div className="endpoint-name">{c.label}</div>
                  <div className="endpoint-meta">{c.detail}</div>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}

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

      {/* Rig Doctor — full machine diagnostics. Nobody gets told "buy a better PC." */}
      <div className="temp-block">
        <h3>Rig Doctor</h3>
        {!fatal && stats === null && <p className="muted">Reading the machine…</p>}
        {!fatal && stats !== null && (
          <>
            <div className="endpoint-meta" style={{ marginBottom: "0.5rem" }}>
              {stats.cpuName} ({stats.cpuCores} cores) &middot; CPU {stats.cpuPct.toFixed(1)}%
              &middot; RAM {stats.memUsedGb.toFixed(1)}/{stats.memTotalGb.toFixed(1)} GB
              ({stats.memPct.toFixed(0)}%)
              {stats.cpuTempC !== null && (
                <> &middot; CPU {stats.cpuTempC.toFixed(1)} °C <span className="muted">({stats.tempSource})</span></>
              )}
            </div>
            {stats.disks.length > 0 && (
              <div className="endpoint-meta" style={{ marginBottom: "0.5rem" }}>
                {stats.disks.map((d) => (
                  <span key={d.mount} style={{ marginRight: "1rem" }}>
                    {d.mount} {d.availableGb.toFixed(1)} GB free ({d.usedPct.toFixed(0)}% used)
                  </span>
                ))}
              </div>
            )}
          </>
        )}
        {!fatal && diagnosis !== null && (
          <ul className="endpoint-list">
            {diagnosis.map((d, i) => (
              <li key={i} className={`endpoint endpoint-${d.level === "ok" ? "pass" : d.level === "fix" ? "unknown" : "warn"}`}>
                <span className={`pill ${d.level === "ok" ? "pill-pass" : d.level === "fix" ? "pill-error" : "pill-warn"}`}>
                  {d.level === "ok" ? "OK" : d.level === "fix" ? "FIX" : "WATCH"}
                </span>
                <div className="endpoint-body">
                  <div className="endpoint-name">{d.headline}</div>
                  <div className="endpoint-meta">{d.detail}</div>
                  {d.fixes.length > 0 && (
                    <ul style={{ margin: "0.25rem 0 0 1rem", padding: 0 }}>
                      {d.fixes.map((f, j) => (
                        <li key={j} className="endpoint-meta" style={{ listStyle: "disc" }}>{f}</li>
                      ))}
                    </ul>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
