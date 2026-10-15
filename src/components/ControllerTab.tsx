import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Mirrors the Rust structs in src-tauri/src/controller.rs (camelCase serde). */
interface DeadzoneInputs {
  leftStickMin: number; leftStickMax: number;
  rightStickMin: number; rightStickMax: number;
  l2Deadzone: number; r2Deadzone: number;
}
interface AimingSettings {
  horizSensitivity: number; vertSensitivity: number;
  responseCurve: string; curveSlopeScale: number;
  adsSensitivityMultiplier: number;
  targetAimAssist: boolean; adsAimAssist: boolean;
  adsTransitionTiming: string; lookInversion: boolean;
}
interface LayoutSettings {
  buttonLayout: string; stickLayoutPreset: string;
  controllerVibration: boolean; triggerEffect: string;
  bumperPing: boolean; swapBumpersTriggers: boolean;
}
interface MovementSettings {
  autoSprint: boolean; slideBehavior: string; cornerSlice: boolean; notes: string;
}
interface ZenValues {
  antirecoilVertical: number; antirecoilHorizontal: number;
  scriptCurve: string; scriptSlot: string; notes: string;
}
interface ControllerRecord {
  id: string; game: string; player: string; recordedAt: string;
  deadzone: DeadzoneInputs; aiming: AimingSettings;
  layout: LayoutSettings; movement: MovementSettings;
  zen: ZenValues | null; notes: string;
}

const blankRecord = (): ControllerRecord => ({
  id: `bo7-${Date.now()}`,
  game: "BO7",
  player: "harry",
  recordedAt: new Date().toISOString().slice(0, 10),
  deadzone: { leftStickMin: 0, leftStickMax: 0, rightStickMin: 0, rightStickMax: 0, l2Deadzone: 0, r2Deadzone: 0 },
  aiming: {
    horizSensitivity: 0, vertSensitivity: 0, responseCurve: "Linear", curveSlopeScale: 0,
    adsSensitivityMultiplier: 1.0, targetAimAssist: true, adsAimAssist: true,
    adsTransitionTiming: "Gradual", lookInversion: false,
  },
  layout: {
    buttonLayout: "Default", stickLayoutPreset: "Default", controllerVibration: false,
    triggerEffect: "Full", bumperPing: false, swapBumpersTriggers: false,
  },
  movement: { autoSprint: false, slideBehavior: "", cornerSlice: false, notes: "" },
  zen: { antirecoilVertical: 37, antirecoilHorizontal: 0, scriptCurve: "", scriptSlot: "1", notes: "" },
  notes: "",
});

function Num(props: { label: string; value: number; onChange: (v: number) => void; step?: number }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      <input
        type="number"
        value={props.value}
        step={props.step ?? 1}
        onChange={(e) => props.onChange(Number(e.target.value))}
      />
    </label>
  );
}

function Txt(props: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      <input type="text" value={props.value} onChange={(e) => props.onChange(e.target.value)} />
    </label>
  );
}

function Bool(props: { label: string; value: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="field field-check">
      <input type="checkbox" checked={props.value} onChange={(e) => props.onChange(e.target.checked)} />
      <span>{props.label}</span>
    </label>
  );
}

export default function ControllerTab() {
  const [records, setRecords] = useState<ControllerRecord[] | null>(null);
  const [editing, setEditing] = useState<ControllerRecord | null>(null);
  const [saving, setSaving] = useState(false);
  const [fatal, setFatal] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setRecords(await invoke<ControllerRecord[]>("list_controller_records"));
    } catch (e) {
      setFatal(e instanceof Error ? e.message : "Could not load controller records.");
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const save = async () => {
    if (!editing) return;
    setSaving(true);
    try {
      setRecords(await invoke<ControllerRecord[]>("save_controller_record", { record: editing }));
      setEditing(null);
    } catch (e) {
      setFatal(typeof e === "string" ? e : e instanceof Error ? e.message : "Save failed.");
    } finally {
      setSaving(false);
    }
  };

  const remove = async (id: string) => {
    try {
      setRecords(await invoke<ControllerRecord[]>("delete_controller_record", { id }));
      if (editing?.id === id) setEditing(null);
    } catch (e) {
      setFatal(typeof e === "string" ? e : e instanceof Error ? e.message : "Delete failed.");
    }
  };

  const set = <K extends keyof ControllerRecord>(k: K, v: ControllerRecord[K]) =>
    setEditing((e) => (e ? { ...e, [k]: v } : e));

  return (
    <section className="card">
      <div className="card-head">
        <h2>Controller — full input chain, per game</h2>
        <button className="btn" onClick={() => setEditing(blankRecord())}>+ New record</button>
      </div>
      <p className="muted">
        Everything between your thumb and the game. The coach reads this to do clean math —
        type it once, it barely changes. Little bro logs CoD settings only; your Zen values stay on your record.
      </p>
      {fatal && <div className="banner banner-error">{fatal}</div>}

      {records !== null && records.length > 0 && !editing && (
        <ul className="endpoint-list">
          {records.map((r) => (
            <li key={r.id} className="endpoint endpoint-pass">
              <div className="endpoint-body">
                <div className="endpoint-name">
                  {r.game} — {r.player === "harry" ? "Harry (Zen)" : "Little bro (raw)"}
                  <span className="default-tag">{r.recordedAt}</span>
                </div>
                <div className="endpoint-meta">
                  Curve {r.aiming.responseCurve} · Deadzones L {r.deadzone.leftStickMin}/{r.deadzone.leftStickMax} R {r.deadzone.rightStickMin}/{r.deadzone.rightStickMax}
                  {r.zen && <> · Zen V/H {r.zen.antirecoilVertical}/{r.zen.antirecoilHorizontal}</>}
                </div>
                <div style={{ marginTop: "0.5rem", display: "flex", gap: "0.5rem" }}>
                  <button className="btn" onClick={() => setEditing({ ...r })}>Edit</button>
                  <button className="btn" onClick={() => void remove(r.id)}>Delete</button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      {records !== null && records.length === 0 && !editing && (
        <p className="muted">No records yet — add your first one.</p>
      )}

      {editing && (
        <div className="editor">
          <div className="editor-grid">
            <Txt label="Game" value={editing.game} onChange={(v) => set("game", v)} />
            <label className="field">
              <span>Player</span>
              <select value={editing.player} onChange={(e) => set("player", e.target.value)}>
                <option value="harry">Harry (Zen on)</option>
                <option value="little-bro">Little bro (raw)</option>
              </select>
            </label>
            <Txt label="Recorded" value={editing.recordedAt} onChange={(v) => set("recordedAt", v)} />
          </div>

          <h3>Deadzone inputs</h3>
          <div className="editor-grid">
            <Num label="L stick min" value={editing.deadzone.leftStickMin} onChange={(v) => set("deadzone", { ...editing.deadzone, leftStickMin: v })} />
            <Num label="L stick max" value={editing.deadzone.leftStickMax} onChange={(v) => set("deadzone", { ...editing.deadzone, leftStickMax: v })} />
            <Num label="R stick min" value={editing.deadzone.rightStickMin} onChange={(v) => set("deadzone", { ...editing.deadzone, rightStickMin: v })} />
            <Num label="R stick max" value={editing.deadzone.rightStickMax} onChange={(v) => set("deadzone", { ...editing.deadzone, rightStickMax: v })} />
            <Num label="L2 deadzone" value={editing.deadzone.l2Deadzone} onChange={(v) => set("deadzone", { ...editing.deadzone, l2Deadzone: v })} />
            <Num label="R2 deadzone" value={editing.deadzone.r2Deadzone} onChange={(v) => set("deadzone", { ...editing.deadzone, r2Deadzone: v })} />
          </div>

          <h3>Aiming</h3>
          <div className="editor-grid">
            <Num label="Horiz sensitivity" value={editing.aiming.horizSensitivity} onChange={(v) => set("aiming", { ...editing.aiming, horizSensitivity: v })} />
            <Num label="Vert sensitivity" value={editing.aiming.vertSensitivity} onChange={(v) => set("aiming", { ...editing.aiming, vertSensitivity: v })} />
            <label className="field">
              <span>Response curve</span>
              <select value={editing.aiming.responseCurve} onChange={(e) => set("aiming", { ...editing.aiming, responseCurve: e.target.value })}>
                <option>Standard</option><option>Linear</option><option>Dynamic</option>
              </select>
            </label>
            <Num label="Curve slope scale" value={editing.aiming.curveSlopeScale} onChange={(v) => set("aiming", { ...editing.aiming, curveSlopeScale: v })} />
            <Num label="ADS sens multiplier" value={editing.aiming.adsSensitivityMultiplier} step={0.05} onChange={(v) => set("aiming", { ...editing.aiming, adsSensitivityMultiplier: v })} />
            <Bool label="Target aim assist" value={editing.aiming.targetAimAssist} onChange={(v) => set("aiming", { ...editing.aiming, targetAimAssist: v })} />
            <Bool label="ADS aim assist" value={editing.aiming.adsAimAssist} onChange={(v) => set("aiming", { ...editing.aiming, adsAimAssist: v })} />
            <Bool label="Look inversion" value={editing.aiming.lookInversion} onChange={(v) => set("aiming", { ...editing.aiming, lookInversion: v })} />
          </div>

          <h3>Layout</h3>
          <div className="editor-grid">
            <Txt label="Button layout" value={editing.layout.buttonLayout} onChange={(v) => set("layout", { ...editing.layout, buttonLayout: v })} />
            <Txt label="Trigger effect" value={editing.layout.triggerEffect} onChange={(v) => set("layout", { ...editing.layout, triggerEffect: v })} />
            <Bool label="Vibration" value={editing.layout.controllerVibration} onChange={(v) => set("layout", { ...editing.layout, controllerVibration: v })} />
            <Bool label="Bumper ping" value={editing.layout.bumperPing} onChange={(v) => set("layout", { ...editing.layout, bumperPing: v })} />
          </div>

          <h3>Movement</h3>
          <div className="editor-grid">
            <Bool label="Auto sprint" value={editing.movement.autoSprint} onChange={(v) => set("movement", { ...editing.movement, autoSprint: v })} />
            <Bool label="Corner slice" value={editing.movement.cornerSlice} onChange={(v) => set("movement", { ...editing.movement, cornerSlice: v })} />
            <Txt label="Slide behavior" value={editing.movement.slideBehavior} onChange={(v) => set("movement", { ...editing.movement, slideBehavior: v })} />
          </div>

          {editing.player === "harry" && (
            <>
              <h3>Zen values (your side only)</h3>
              <div className="editor-grid">
                <Num label="Anti-recoil vertical" value={editing.zen?.antirecoilVertical ?? 0} onChange={(v) => set("zen", { ...(editing.zen ?? blankRecord().zen!), antirecoilVertical: v })} />
                <Num label="Anti-recoil horizontal" value={editing.zen?.antirecoilHorizontal ?? 0} onChange={(v) => set("zen", { ...(editing.zen ?? blankRecord().zen!), antirecoilHorizontal: v })} />
                <Txt label="Script curve" value={editing.zen?.scriptCurve ?? ""} onChange={(v) => set("zen", { ...(editing.zen ?? blankRecord().zen!), scriptCurve: v })} />
                <Txt label="Script slot" value={editing.zen?.scriptSlot ?? ""} onChange={(v) => set("zen", { ...(editing.zen ?? blankRecord().zen!), scriptSlot: v })} />
              </div>
            </>
          )}

          <h3>Notes</h3>
          <label className="field">
            <textarea value={editing.notes} onChange={(e) => set("notes", e.target.value)} rows={2} style={{ width: "100%" }} />
          </label>

          <div style={{ display: "flex", gap: "0.5rem", marginTop: "1rem" }}>
            <button className="btn" onClick={() => void save()} disabled={saving}>
              {saving ? "Saving…" : "Save record"}
            </button>
            <button className="btn" onClick={() => setEditing(null)}>Cancel</button>
          </div>
        </div>
      )}
    </section>
  );
}
