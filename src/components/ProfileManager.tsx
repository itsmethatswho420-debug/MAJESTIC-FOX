import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Profile Manager — the "many profiles" half of "One script, many profiles".
 *
 * The Zen runs ONE script (Harry's Fox custom). This tab is the profile
 * library: one profile per weapon/game holding that weapon's tuning values.
 *
 * v0.2.0: Harry can add, edit, and delete his own profiles. They are stored
 * in the app-data dir (`profiles.json`) as the "user" layer; the bundled
 * seed profiles are read-only. Editing a seed saves a user-layer override
 * with the same id (the seeds themselves are never modified). Deleting a
 * user override of a seed id restores the seed's original values.
 *
 * v0.3.0: Export to Zen. The app cannot push values into a running Zen
 * script (GPC has no PC-to-script channel), so "Export for Zen" renders a
 * clean Fox V3 `.gpc` with the profile values baked in, ready to compile
 * in Zen Studio. The button says exactly what it does — never "Push".
 */

/** Mirrors `ProfileValues` in src-tauri/src/profiles.rs (camelCase serde). */
interface ProfileValues {
  antirecoilVertical: number;
  antirecoilHorizontal: number;
  stickDeadzone: number;
  responseCurve: string;
}

/** Mirrors `Profile` in src-tauri/src/profiles.rs. */
interface Profile {
  id: string;
  game: string;
  weapon: string;
  notes: string;
  values: ProfileValues;
  /** "seed" (bundled, read-only) or "user" (Harry's own). Stamped by the backend. */
  source: string;
}

const ACTIVE_KEY = "majestic-fox:active-profile-id";

const VALUE_ROWS: { key: keyof ProfileValues; label: string; hint: string }[] = [
  {
    key: "antirecoilVertical",
    label: "Anti-recoil vertical",
    hint: "0-100 — same scale as the Zen quick-nudge",
  },
  {
    key: "antirecoilHorizontal",
    label: "Anti-recoil horizontal",
    hint: "0-100 — same scale as the Zen quick-nudge",
  },
  {
    key: "stickDeadzone",
    label: "Stick deadzone",
    hint: "0-100",
  },
  {
    key: "responseCurve",
    label: "Response curve",
    hint: "e.g. linear / dynamic / aggressive",
  },
];

const NUMERIC_KEYS: (keyof ProfileValues)[] = [
  "antirecoilVertical",
  "antirecoilHorizontal",
  "stickDeadzone",
];

interface FormState {
  game: string;
  weapon: string;
  notes: string;
  antirecoilVertical: string;
  antirecoilHorizontal: string;
  stickDeadzone: string;
  responseCurve: string;
}

const EMPTY_FORM: FormState = {
  game: "",
  weapon: "",
  notes: "",
  antirecoilVertical: "40",
  antirecoilHorizontal: "0",
  stickDeadzone: "3",
  responseCurve: "dynamic",
};

/** Make a stable, URL-safe id for a new profile: "warzone-assault-rifle-a3f9". */
function makeId(game: string, weapon: string): string {
  const slug = (s: string) =>
    s
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40) || "profile";
  const rand = Math.floor(Math.random() * 0xffff)
    .toString(16)
    .padStart(4, "0");
  return `${slug(game)}-${slug(weapon)}-${rand}`;
}

function formFromProfile(p: Profile): FormState {
  return {
    game: p.game,
    weapon: p.weapon,
    notes: p.notes,
    antirecoilVertical: String(p.values.antirecoilVertical),
    antirecoilHorizontal: String(p.values.antirecoilHorizontal),
    stickDeadzone: String(p.values.stickDeadzone),
    responseCurve: p.values.responseCurve,
  };
}

export default function ProfileManager() {
  const [profiles, setProfiles] = useState<Profile[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(() => {
    try {
      return window.localStorage.getItem(ACTIVE_KEY);
    } catch {
      return null;
    }
  });
  const [fatal, setFatal] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportPath, setExportPath] = useState<string | null>(null);
  // null = no form open; { mode: "new" } or { mode: "edit", id } otherwise.
  const [form, setForm] = useState<{ mode: "new" | "edit"; id: string | null; state: FormState } | null>(null);

  const load = useCallback(async () => {
    setFatal(null);
    try {
      const list = await invoke<Profile[]>("list_profiles");
      setProfiles(list);
      if (list.length > 0 && selectedId === null) {
        setSelectedId(list[0].id);
      }
      // If the selected profile vanished (deleted), fall back to the first.
      if (selectedId !== null && !list.some((p) => p.id === selectedId)) {
        setSelectedId(list.length > 0 ? list[0].id : null);
      }
    } catch (e) {
      // invoke() rejects outside the Tauri webview (plain browser) or when
      // the Rust command returns Err — either way, say so plainly.
      setFatal(
        e instanceof Error
          ? e.message
          : typeof e === "string"
            ? e
            : "Could not reach the profile backend. Are you running inside the Tauri app?",
      );
      setProfiles(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const setActive = (id: string) => {
    setActiveId(id);
    try {
      window.localStorage.setItem(ACTIVE_KEY, id);
    } catch {
      // Private-mode storage failure: the in-memory state still works for
      // this session; persistence is a convenience, not a requirement.
    }
  };

  const clearActiveIfDeleted = (id: string) => {
    if (activeId === id) {
      setActiveId(null);
      try {
        window.localStorage.removeItem(ACTIVE_KEY);
      } catch {
        // ignore
      }
    }
  };

  const openNewForm = () => {
    setFormError(null);
    setForm({ mode: "new", id: null, state: { ...EMPTY_FORM } });
  };

  const openEditForm = (p: Profile) => {
    setFormError(null);
    setForm({ mode: "edit", id: p.id, state: formFromProfile(p) });
  };

  const closeForm = () => {
    setForm(null);
    setFormError(null);
  };

  const setField = (key: keyof FormState, value: string) => {
    setForm((f) => (f === null ? f : { ...f, state: { ...f.state, [key]: value } }));
  };

  const saveForm = async () => {
    if (form === null || saving) return;
    setFormError(null);

    const st = form.state;
    const game = st.game.trim();
    const weapon = st.weapon.trim();
    if (!game || !weapon) {
      setFormError("Give the profile a game and a weapon name.");
      return;
    }

    const nums: Record<string, number> = {};
    for (const key of NUMERIC_KEYS) {
      const raw = st[key].trim();
      const n = Number(raw);
      if (!Number.isInteger(n) || n < 0 || n > 100) {
        setFormError(`"${raw || "—"}" isn't a whole number from 0 to 100.`);
        return;
      }
      nums[key] = n;
    }
    const curve = st.responseCurve.trim() || "dynamic";

    // New profiles get a fresh id. Edits keep the original id — for a seed
    // id that means "save my override of the built-in profile".
    const id = form.mode === "new" ? makeId(game, weapon) : (form.id as string);

    const profile: Profile = {
      id,
      game,
      weapon,
      notes: st.notes.trim(),
      values: {
        antirecoilVertical: nums.antirecoilVertical,
        antirecoilHorizontal: nums.antirecoilHorizontal,
        stickDeadzone: nums.stickDeadzone,
        responseCurve: curve,
      },
      source: "user",
    };

    setSaving(true);
    try {
      const saved = await invoke<Profile>("save_profile", { profile });
      await load();
      setSelectedId(saved.id);
      closeForm();
    } catch (e) {
      setFormError(
        e instanceof Error ? e.message : typeof e === "string" ? e : "Could not save the profile.",
      );
    } finally {
      setSaving(false);
    }
  };

  const deleteSelected = async (p: Profile) => {
    if (p.source !== "user") return;
    const label = `${p.game} — ${p.weapon}`;
    if (!window.confirm(`Delete your profile "${label}"? Built-in profiles can't be deleted, but this one is yours.`)) {
      return;
    }
    setFatal(null);
    try {
      await invoke<boolean>("delete_profile", { id: p.id });
      clearActiveIfDeleted(p.id);
      await load();
    } catch (e) {
      setFatal(
        e instanceof Error ? e.message : typeof e === "string" ? e : "Could not delete the profile.",
      );
    }
  };

  const selected = profiles?.find((p) => p.id === selectedId) ?? null;
  const active = profiles?.find((p) => p.id === activeId) ?? null;

  /** Export the selected profile (or all profiles) as a Fox V3 `.gpc`. */
  const exportZen = async (profileId: string | null) => {
    if (exporting) return;
    setExporting(true);
    setFatal(null);
    try {
      const path = await invoke<string>("export_zen_gpc", { profileId });
      setExportPath(path);
    } catch (e) {
      setFatal(
        e instanceof Error ? e.message : typeof e === "string" ? e : "Could not export the script.",
      );
      setExportPath(null);
    } finally {
      setExporting(false);
    }
  };

  return (
    <section className="card">
      <div className="card-head">
        <h2>Profiles &mdash; one script, many profiles</h2>
        <div className="card-head-actions">
          <button className="btn btn-primary" onClick={openNewForm}>
            + New profile
          </button>
          <button className="btn" onClick={() => void load()}>
            Reload
          </button>
        </div>
      </div>

      {fatal && <div className="banner banner-error">{fatal}</div>}

      {!fatal && active && (
        <div className="banner banner-ok">
          Active profile: <strong>{active.game} &mdash; {active.weapon}</strong>.
          Dial these numbers in on the Zen with the quick-nudge (hold ADS + D-pad).
        </div>
      )}

      {!fatal && !active && profiles !== null && profiles.length > 0 && (
        <div className="banner banner-warn">
          No active profile selected. Pick one below to make it your active setup.
        </div>
      )}

      <div className="profile-grid">
        {(profiles ?? []).map((p) => {
          const isActive = p.id === activeId;
          const isSelected = p.id === selectedId;
          return (
            <button
              key={p.id}
              type="button"
              className={`profile-card${isSelected ? " profile-card-selected" : ""}`}
              onClick={() => setSelectedId(p.id)}
            >
              <span className={`pill ${isActive ? "pill-pass" : "pill-unknown"}`}>
                {isActive ? "ACTIVE" : p.game.toUpperCase()}
              </span>
              <div className="profile-card-title">{p.weapon}</div>
              <div className="profile-card-id muted">
                {p.source === "user" ? "yours" : "built-in"}
              </div>
            </button>
          );
        })}
      </div>

      {!fatal && profiles !== null && profiles.length === 0 && (
        <p className="muted">No profiles found. The seed file may be missing.</p>
      )}

      {form !== null && (
        <div className="profile-form">
          <h3>{form.mode === "new" ? "New profile" : "Edit profile"}</h3>
          {form.mode === "edit" &&
            (profiles?.find((p) => p.id === form.id)?.source ?? "seed") === "seed" && (
              <p className="muted">
                This is a built-in profile — your edit is saved as <em>your own copy</em>;
                the built-in stays untouched underneath.
              </p>
            )}
          {formError && <div className="banner banner-error">{formError}</div>}
          <div className="form-grid">
            <label className="form-field">
              <span>Game</span>
              <input
                type="text"
                value={form.state.game}
                onChange={(e) => setField("game", e.target.value)}
                placeholder="Warzone"
              />
            </label>
            <label className="form-field">
              <span>Weapon</span>
              <input
                type="text"
                value={form.state.weapon}
                onChange={(e) => setField("weapon", e.target.value)}
                placeholder="Assault Rifle"
              />
            </label>
            <label className="form-field form-field-wide">
              <span>Notes</span>
              <input
                type="text"
                value={form.state.notes}
                onChange={(e) => setField("notes", e.target.value)}
                placeholder="What this profile is for…"
              />
            </label>
            {VALUE_ROWS.filter((r) => r.key !== "responseCurve").map((row) => (
              <label className="form-field" key={row.key}>
                <span>{row.label} (0–100)</span>
                <input
                  type="number"
                  min={0}
                  max={100}
                  step={1}
                  value={form.state[row.key]}
                  onChange={(e) => setField(row.key as keyof FormState, e.target.value)}
                />
              </label>
            ))}
            <label className="form-field">
              <span>Response curve</span>
              <input
                type="text"
                value={form.state.responseCurve}
                onChange={(e) => setField("responseCurve", e.target.value)}
                placeholder="linear / dynamic / aggressive"
              />
            </label>
          </div>
          <div className="profile-actions">
            <button type="button" className="btn btn-primary" onClick={() => void saveForm()} disabled={saving}>
              {saving ? "Saving…" : form.mode === "new" ? "Save profile" : "Save my copy"}
            </button>
            <button type="button" className="btn" onClick={closeForm} disabled={saving}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {selected && form === null && (
        <div className="profile-detail">
          <h3>
            {selected.game} &mdash; {selected.weapon}
          </h3>
          <p className="muted">
            {selected.notes}
            {selected.source === "user" && (
              <>
                {" "}
                <span className="pill pill-pass">YOURS</span>
              </>
            )}
          </p>
          <ul className="value-list">
            {VALUE_ROWS.map((row) => (
              <li key={row.key} className="value-row">
                <div>
                  <div className="value-label">{row.label}</div>
                  <div className="value-hint">{row.hint}</div>
                </div>
                <div className="value-number">{selected.values[row.key]}</div>
              </li>
            ))}
          </ul>
          <div className="profile-actions">
            <button
              type="button"
              className="btn"
              onClick={() => setActive(selected.id)}
              disabled={selected.id === activeId}
            >
              {selected.id === activeId ? "Active profile" : "Set as active"}
            </button>
            <button type="button" className="btn" onClick={() => openEditForm(selected)}>
              Edit
            </button>
            {selected.source === "user" && (
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => void deleteSelected(selected)}
              >
                Delete
              </button>
            )}
            {/* Export for Zen: renders a Fox V3 .gpc with the values baked in.
                Honest label — it generates a file, it does not transmit. */}
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void exportZen(selected.id)}
              disabled={exporting}
              title="Generate a Fox V3 .gpc script with this profile's values baked in"
            >
              {exporting ? "Exporting…" : "Export for Zen"}
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => void exportZen(null)}
              disabled={exporting}
              title="Generate one Fox V3 .gpc with every profile as a switchable preset"
            >
              {exporting ? "Exporting…" : "Export all for Zen"}
            </button>
          </div>
          <p className="muted">
            Export writes a Fox V3 script file with your numbers baked in — then
            load it in Zen Studio (Compiler &rarr; open file &rarr; Compile &rarr;
            drag to a memory slot &rarr; Program). Your Fox V2 custom script is
            untouched. You can still fine-tune on the device: hold ADS + D-pad.
          </p>
        </div>
      )}

      {exportPath && (
        <div className="banner banner-ok">
          Script written: <strong>{exportPath}</strong>
          <br />
          In Zen Studio: Compiler tab &rarr; open this file &rarr; Compile &rarr;
          drag to a memory slot &rarr; Program. On the controller: hold VIEW +
          D-pad LEFT/RIGHT to switch weapon presets.
        </div>
      )}
    </section>
  );
}
