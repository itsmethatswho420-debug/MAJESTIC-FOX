import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Profile Manager — the "many profiles" half of "One script, many profiles".
 *
 * The Zen runs ONE script (Harry's Fox custom). This tab is the profile
 * library: one profile per weapon/game holding that weapon's tuning values.
 *
 * TODO (later milestone): push the active profile's values to the Zen
 * automatically (USB/serial link from the app to the Zen). Until that link
 * exists, the workflow is manual: read the numbers here, dial them in with
 * the Zen quick-nudge (hold ADS + D-pad UP/DOWN on the Fox custom script,
 * same 0-100 scale). The "Push to Zen" button below is disabled and marked
 * "coming soon" until then — it must never look like it works when it doesn't.
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

  const load = useCallback(async () => {
    setFatal(null);
    try {
      const list = await invoke<Profile[]>("list_profiles");
      setProfiles(list);
      if (list.length > 0 && selectedId === null) {
        setSelectedId(list[0].id);
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

  const selected = profiles?.find((p) => p.id === selectedId) ?? null;
  const active = profiles?.find((p) => p.id === activeId) ?? null;

  return (
    <section className="card">
      <div className="card-head">
        <h2>Profiles &mdash; one script, many profiles</h2>
        <button className="btn" onClick={() => void load()}>
          Reload
        </button>
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
              <div className="profile-card-id muted">{p.id}</div>
            </button>
          );
        })}
      </div>

      {!fatal && profiles !== null && profiles.length === 0 && (
        <p className="muted">No profiles found. The seed file may be missing.</p>
      )}

      {selected && (
        <div className="profile-detail">
          <h3>
            {selected.game} &mdash; {selected.weapon}
          </h3>
          <p className="muted">{selected.notes}</p>
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
            {/* TODO: wire to the Zen link milestone. Disabled + labeled so it
                can never be mistaken for working. */}
            <button
              type="button"
              className="btn"
              disabled
              title="Coming soon — automatic push to the Zen is a later milestone"
            >
              Push to Zen &mdash; coming soon
            </button>
          </div>
          <p className="muted">
            For now: read the numbers above, then dial them in on the Zen with the
            quick-nudge (hold ADS, tap D-pad UP/DOWN for vertical, LEFT/RIGHT for
            horizontal). Same 0-100 scale.
          </p>
        </div>
      )}
    </section>
  );
}
