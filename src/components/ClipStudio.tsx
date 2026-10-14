import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

/**
 * Clip-to-Social studio tab (spec: majestic-fox-clip-to-social-spec.md).
 *
 * Pick a stream recording, mark In/Out ranges, and export each as a
 * 1080x1920 vertical clip named with Harry's TikTok convention
 * (`TikTok - YYYY-MM-DD - {title}.mp4`).
 *
 * Non-negotiables (per spec):
 * - Nothing uploads or posts anywhere. Export is local only.
 * - Every export happens on an explicit per-clip Export tap (the approval gate).
 * - Captions: manual SRT upload per clip for now. Auto-transcription is a
 *   future hook, NOT this build.
 * - Future hooks not built here: purple/dark grade treatment, handle
 *   lower-third, auto-suggested highlights, posting queue / destinations.
 */

/** Mirrors `Recording` in src-tauri/src/clips.rs (camelCase serde). */
interface Recording {
  name: string;
  path: string;
  sizeMb: number;
  durationSec: number;
}

/** Mirrors `Clip` in src-tauri/src/clips.rs. */
interface Clip {
  id: string;
  recordingPath: string;
  recordingName: string;
  inSec: number;
  outSec: number;
  title: string;
  status: "draft" | "queued" | "exporting" | "done" | "error";
  outputPath?: string | null;
  error?: string | null;
  createdAt: string;
}

/** Mirrors `ExportResult` in src-tauri/src/clips.rs. */
interface ExportResult {
  outputPath: string;
  driveName: string;
  durationSec: number;
}

const STATUS_PILL: Record<Clip["status"], string> = {
  draft: "pill-unknown",
  queued: "pill-warn",
  exporting: "pill-warn",
  done: "pill-pass",
  error: "pill-error",
};

const STATUS_LABEL: Record<Clip["status"], string> = {
  draft: "DRAFT",
  queued: "QUEUED",
  exporting: "WORKING",
  done: "DONE",
  error: "ERROR",
};

function fmtTime(sec: number): string {
  if (!isFinite(sec) || sec < 0) return "0:00.0";
  const m = Math.floor(sec / 60);
  const s = sec - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, "0")}`;
}

function fmtDur(sec: number): string {
  if (!sec || sec <= 0) return "—";
  return fmtTime(sec);
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Fallback for contexts where the async clipboard API is unavailable.
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      document.body.removeChild(ta);
      return ok;
    } catch {
      return false;
    }
  }
}

export default function ClipStudio() {
  const [recordings, setRecordings] = useState<Recording[] | null>(null);
  const [selectedPath, setSelectedPath] = useState<string>("");
  const [clips, setClips] = useState<Clip[]>([]);
  const [inMark, setInMark] = useState<number | null>(null);
  const [outMark, setOutMark] = useState<number | null>(null);
  const [draftTitle, setDraftTitle] = useState("");
  const [srtText, setSrtText] = useState<string | null>(null);
  const [srtName, setSrtName] = useState<string | null>(null);
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [fatal, setFatal] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const srtInputRef = useRef<HTMLInputElement>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setFatal(null);
    try {
      const [recs, saved] = await Promise.all([
        invoke<Recording[]>("list_recordings"),
        invoke<Clip[]>("list_clips"),
      ]);
      setRecordings(recs);
      setClips(saved);
      if (recs.length > 0 && !selectedPath) {
        setSelectedPath(recs[0].path);
      }
    } catch (e) {
      setFatal(
        e instanceof Error
          ? e.message
          : typeof e === "string"
            ? e
            : "Could not reach the clip backend. Are you running inside the Tauri app?",
      );
      setRecordings(null);
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const videoSrc = selectedPath ? convertFileSrc(selectedPath) : "";
  const selected = recordings?.find((r) => r.path === selectedPath) ?? null;

  const now = (): number | null => {
    const v = videoRef.current;
    if (!v || !isFinite(v.currentTime)) return null;
    return v.currentTime;
  };

  const markIn = () => {
    const t = now();
    if (t !== null) setInMark(t);
  };
  const markOut = () => {
    const t = now();
    if (t !== null) setOutMark(t);
  };
  const nudge = (delta: number) => {
    const v = videoRef.current;
    if (v && isFinite(v.duration)) {
      v.currentTime = Math.min(Math.max(0, v.currentTime + delta), v.duration);
    }
  };
  const clearMarks = () => {
    setInMark(null);
    setOutMark(null);
  };

  const onSrtFile = (file: File | undefined) => {
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      setSrtText(typeof reader.result === "string" ? reader.result : null);
      setSrtName(file.name);
    };
    reader.onerror = () => {
      setNotice(`Could not read ${file.name} — captions skipped.`);
      setSrtText(null);
      setSrtName(null);
    };
    reader.readAsText(file);
  };

  const marksValid =
    inMark !== null && outMark !== null && outMark > inMark && selectedPath !== "";

  const addClip = async () => {
    if (!marksValid || !selected) return;
    if (draftTitle.trim() === "") {
      setNotice("Give the clip a title first.");
      return;
    }
    setNotice(null);
    try {
      const saved = await invoke<Clip>("save_clip", {
        clip: {
          id: "",
          recordingPath: selected.path,
          recordingName: selected.name,
          inSec: inMark,
          outSec: outMark,
          title: draftTitle.trim(),
          status: "draft",
          outputPath: null,
          error: null,
          createdAt: "",
        },
      });
      setClips((prev) => [saved, ...prev]);
      setDraftTitle("");
      clearMarks();
      setNotice(`Clip "${saved.title}" saved as a draft.`);
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e));
    }
  };

  const setBusy = (id: string, on: boolean) => {
    setBusyIds((prev) => {
      const next = new Set(prev);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const patchClip = async (clip: Clip, patch: Partial<Clip>): Promise<Clip> => {
    const updated = await invoke<Clip>("save_clip", { clip: { ...clip, ...patch } });
    setClips((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
    return updated;
  };

  const exportOne = async (clip: Clip) => {
    if (busyIds.has(clip.id)) return;
    setBusy(clip.id, true);
    setNotice(null);
    try {
      await patchClip(clip, { status: "exporting", error: null, outputPath: null });
      const result = await invoke<ExportResult>("export_clip", {
        inputPath: clip.recordingPath,
        inSec: clip.inSec,
        outSec: clip.outSec,
        title: clip.title,
        captionsSrt: srtText,
      });
      await patchClip(clip, { status: "done", outputPath: result.outputPath, error: null });
      setNotice(`Exported: ${result.driveName}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      await patchClip(clip, { status: "error", error: msg });
      setNotice(msg);
    } finally {
      setBusy(clip.id, false);
    }
  };

  const deleteOne = async (clip: Clip) => {
    if (busyIds.has(clip.id)) return;
    setBusy(clip.id, true);
    try {
      await invoke<boolean>("delete_clip", { id: clip.id });
      setClips((prev) => prev.filter((c) => c.id !== clip.id));
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(clip.id, false);
    }
  };

  const copyDriveName = async (clip: Clip) => {
    if (!clip.outputPath) return;
    const ok = await copyText(baseName(clip.outputPath));
    setNotice(ok ? `Copied: ${baseName(clip.outputPath)}` : "Copy failed — select the path text manually.");
  };

  return (
    <section className="card">
      <div className="card-head">
        <h2>Clip studio &mdash; recordings to vertical clips</h2>
        <button className="btn" onClick={() => void refresh()} disabled={loading}>
          {loading ? "Loading…" : "Re-scan"}
        </button>
      </div>

      {fatal && <div className="banner banner-error">{fatal}</div>}
      {notice && !fatal && <div className="banner banner-warn">{notice}</div>}

      <p className="muted">
        Exports stay on this machine as <code>TikTok - YYYY-MM-DD - title.mp4</code>. Nothing
        uploads or posts &mdash; each destination needs your tap later.
      </p>

      {/* Recording picker */}
      <div className="form-grid">
        <div className="form-field form-field-wide">
          <label className="value-label" htmlFor="clip-recording">
            Recording
          </label>
          <select
            id="clip-recording"
            value={selectedPath}
            disabled={!recordings || recordings.length === 0}
            onChange={(e) => {
              setSelectedPath(e.target.value);
              clearMarks();
            }}
            style={{
              padding: "0.5rem 0.6rem",
              borderRadius: 8,
              border: "1px solid rgba(255,255,255,0.18)",
              background: "rgba(0,0,0,0.35)",
              color: "inherit",
              fontSize: "1rem",
              width: "100%",
            }}
          >
            {(recordings ?? []).map((r) => (
              <option key={r.path} value={r.path}>
                {r.name} &middot; {fmtDur(r.durationSec)} &middot; {r.sizeMb.toFixed(1)} MB
              </option>
            ))}
          </select>
          {recordings !== null && recordings.length === 0 && !fatal && (
            <p className="muted">
              No recordings found. Drop .mp4 files in the app&apos;s <code>recordings</code>{" "}
              folder, then Re-scan.
            </p>
          )}
        </div>
      </div>

      {/* Player + marks */}
      {selectedPath && (
        <>
          <video
            key={videoSrc}
            ref={videoRef}
            src={videoSrc}
            controls
            preload="metadata"
            style={{ width: "100%", maxHeight: 380, borderRadius: 8, background: "#000" }}
          />
          <div className="profile-actions" style={{ marginTop: "0.75rem" }}>
            <button className="btn btn-primary" onClick={markIn}>
              Mark In{inMark !== null ? ` — ${fmtTime(inMark)}` : ""}
            </button>
            <button className="btn btn-primary" onClick={markOut}>
              Mark Out{outMark !== null ? ` — ${fmtTime(outMark)}` : ""}
            </button>
            <button className="btn" onClick={clearMarks}>
              Clear
            </button>
          </div>
          <div className="profile-actions" style={{ marginTop: "0.5rem" }}>
            <span className="muted">Nudge:</span>
            <button className="btn" onClick={() => nudge(-1)}>&minus;1s</button>
            <button className="btn" onClick={() => nudge(-1 / 30)}>&minus;1f</button>
            <button className="btn" onClick={() => nudge(1 / 30)}>+1f</button>
            <button className="btn" onClick={() => nudge(1)}>+1s</button>
            {inMark !== null && outMark !== null && (
              <span className="muted">
                Range: {fmtTime(inMark)} &rarr; {fmtTime(outMark)} ({fmtDur(outMark - inMark)})
              </span>
            )}
          </div>

          {/* New clip draft */}
          <div className="form-grid" style={{ marginTop: "0.75rem" }}>
            <div className="form-field">
              <label className="value-label" htmlFor="clip-title">
                Clip title
              </label>
              <input
                id="clip-title"
                value={draftTitle}
                onChange={(e) => setDraftTitle(e.target.value)}
                placeholder="Short title for the file name"
                maxLength={60}
              />
            </div>
            <div className="form-field">
              <label className="value-label" htmlFor="clip-srt">
                Captions (.srt, optional)
              </label>
              <input
                id="clip-srt"
                ref={srtInputRef}
                type="file"
                accept=".srt"
                onChange={(e) => onSrtFile(e.target.files?.[0])}
                style={{
                  padding: "0.5rem 0.6rem",
                  borderRadius: 8,
                  border: "1px solid rgba(255,255,255,0.18)",
                  background: "rgba(0,0,0,0.35)",
                  color: "inherit",
                  fontSize: "0.9rem",
                  width: "100%",
                }}
              />
              {srtName && <p className="muted">{srtName} loaded &mdash; burns in on export.</p>}
            </div>
          </div>
          <div className="profile-actions" style={{ marginTop: "0.5rem" }}>
            <button className="btn btn-primary" onClick={() => void addClip()} disabled={!marksValid}>
              Add clip
            </button>
            {!marksValid && (
              <span className="muted">Mark an In and an Out on the timeline first.</span>
            )}
          </div>
        </>
      )}

      {/* Clip list / export queue */}
      <h3 style={{ marginTop: "1.25rem" }}>Clips ({clips.length})</h3>
      {clips.length === 0 ? (
        <p className="muted">No clips yet. Mark a range above and add your first one.</p>
      ) : (
        <ul className="endpoint-list">
          {clips.map((clip) => {
            const busy = busyIds.has(clip.id);
            return (
              <li key={clip.id} className="endpoint">
                <span className={`pill ${STATUS_PILL[clip.status]}`}>
                  {busy ? "…" : STATUS_LABEL[clip.status]}
                </span>
                <div className="endpoint-body">
                  <div className="endpoint-name">{clip.title}</div>
                  <div className="endpoint-meta">
                    {clip.recordingName} &middot; {fmtTime(clip.inSec)} &rarr; {fmtTime(clip.outSec)}{" "}
                    ({fmtDur(clip.outSec - clip.inSec)})
                    {clip.outputPath && (
                      <>
                        <br />
                        <span className="fix-hint">{baseName(clip.outputPath)}</span>
                      </>
                    )}
                    {clip.error && (
                      <>
                        <br />
                        <span className="fix-hint">{clip.error}</span>
                      </>
                    )}
                  </div>
                  <div className="profile-actions" style={{ marginTop: "0.5rem" }}>
                    {(clip.status === "draft" || clip.status === "error") && (
                      <button
                        className="btn btn-primary"
                        disabled={busy}
                        onClick={() => void exportOne(clip)}
                      >
                        {busy ? "Exporting…" : "Export 1080×1920"}
                      </button>
                    )}
                    {clip.status === "done" && clip.outputPath && (
                      <button className="btn" onClick={() => void copyDriveName(clip)}>
                        Copy Drive name
                      </button>
                    )}
                    <button className="btn btn-danger" disabled={busy} onClick={() => void deleteOne(clip)}>
                      Delete
                    </button>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
