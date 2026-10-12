import { useState } from "react";
import HealthDashboard from "./components/HealthDashboard";
import ProfileManager from "./components/ProfileManager";
import MusePanel from "./components/MusePanel";

type Tab = "health" | "profiles" | "muse";

const TABS: { id: Tab; label: string }[] = [
  { id: "health", label: "Health" },
  { id: "profiles", label: "Profiles" },
  { id: "muse", label: "Muse" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("health");

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <img className="brand-mark" src="emblem.png" alt="" aria-hidden="true" />
          <div>
            <h1>Majestic Fox</h1>
            <p className="subtitle">Stream co-pilot &mdash; rig check &amp; script profiles</p>
          </div>
        </div>
        <nav className="tabs" aria-label="Sections">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`tab${tab === t.id ? " tab-active" : ""}`}
              onClick={() => setTab(t.id)}
              aria-current={tab === t.id ? "page" : undefined}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>
      <main>
        {tab === "health" ? (
          <HealthDashboard />
        ) : tab === "profiles" ? (
          <ProfileManager />
        ) : (
          <MusePanel />
        )}
      </main>
      <footer className="app-footer">
        <span>Phase 1: rig-health checks + profile library &middot; featherweight build for the Dell</span>
      </footer>
    </div>
  );
}
