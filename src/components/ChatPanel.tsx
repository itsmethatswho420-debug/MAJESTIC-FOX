import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface XPost {
  id: string;
  author: string;
  text: string;
  createdAt: string;
}

const TOKEN_KEY = "majestic-fox-x-bearer";
const USER_KEY = "majestic-fox-x-username";

export default function ChatPanel() {
  const [bearer, setBearer] = useState("");
  const [username, setUsername] = useState("");
  const [userId, setUserId] = useState<string | null>(null);
  const [posts, setPosts] = useState<XPost[]>([]);
  const [sinceId, setSinceId] = useState<string | null>(null);
  const [polling, setPolling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    setBearer(localStorage.getItem(TOKEN_KEY) ?? "");
    setUsername(localStorage.getItem(USER_KEY) ?? "");
    return () => {
      if (timer.current) window.clearInterval(timer.current);
    };
  }, []);

  const poll = useCallback(
    async (uid: string, token: string, sid: string | null) => {
      try {
        const [fresh, newest] = await invoke<[XPost[], string | null]>("x_poll_mentions", {
          bearer: token,
          userId: uid,
          sinceId: sid,
        });
        if (fresh.length > 0) {
          setPosts((p) => [...fresh.reverse(), ...p].slice(0, 200));
        }
        if (newest) setSinceId(newest);
        setError(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : "Poll failed.");
        setPolling(false);
        if (timer.current) window.clearInterval(timer.current);
      }
    },
    [],
  );

  const connect = async () => {
    setError(null);
    const token = bearer.trim();
    const name = username.trim().replace(/^@/, "");
    if (!token || !name) {
      setError("Paste your X Bearer token and your handle first.");
      return;
    }
    try {
      const uid = await invoke<string>("x_resolve_user", { bearer: token, username: name });
      setUserId(uid);
      setConnected(true);
      localStorage.setItem(TOKEN_KEY, token);
      localStorage.setItem(USER_KEY, name);
      await poll(uid, token, null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not connect to X.");
      setConnected(false);
    }
  };

  const togglePolling = () => {
    if (polling) {
      if (timer.current) window.clearInterval(timer.current);
      setPolling(false);
      return;
    }
    if (!userId) return;
    setPolling(true);
    // Every 2 minutes — keeps pay-per-use costs sane (~$0.005/read).
    timer.current = window.setInterval(() => {
      void poll(userId, bearer.trim(), sinceId);
    }, 120_000);
    void poll(userId, bearer.trim(), sinceId);
  };

  const disconnect = () => {
    if (timer.current) window.clearInterval(timer.current);
    setPolling(false);
    setConnected(false);
    setUserId(null);
    setPosts([]);
    setSinceId(null);
  };

  return (
    <section className="card">
      <div className="card-head">
        <h2>Chat — X mentions</h2>
        {connected && (
          <button className="btn" onClick={togglePolling}>
            {polling ? "Stop live poll" : "Start live poll"}
          </button>
        )}
      </div>
      <p className="muted">
        Your X mentions and replies, in the dashboard — not a separate app. Polls every 2 minutes
        while live. <strong>Cost:</strong> X charges ~$0.005 per read now (no free tier) — a 4-hour
        stream runs a few dollars in API credits. Your token stays on this machine, sent only to
        api.x.com. TikTok/Twitch chat join this panel with the studio phase.
      </p>

      {!connected && (
        <div className="editor-grid" style={{ marginBottom: "1rem" }}>
          <label className="field">
            <span>X Bearer token (from developer.x.com)</span>
            <input
              type="password"
              value={bearer}
              onChange={(e) => setBearer(e.target.value)}
              placeholder="Paste Bearer token"
            />
          </label>
          <label className="field">
            <span>Your X handle</span>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="@handle"
            />
          </label>
          <div>
            <button className="btn" onClick={() => void connect()}>
              Connect X
            </button>
          </div>
        </div>
      )}

      {connected && (
        <div style={{ marginBottom: "1rem", display: "flex", gap: "0.5rem", alignItems: "center" }}>
          <span className="pill pill-pass">Connected as @{username}</span>
          <button className="btn" onClick={disconnect}>
            Disconnect
          </button>
          {polling && <span className="muted">Live — polling every 2 min</span>}
        </div>
      )}

      {error && <div className="banner banner-error">{error}</div>}

      {connected && posts.length === 0 && !error && (
        <p className="muted">No mentions yet — they'll land here when people @ you.</p>
      )}

      <ul className="endpoint-list">
        {posts.map((p) => (
          <li key={p.id} className="endpoint endpoint-pass">
            <div className="endpoint-body">
              <div className="endpoint-name">{p.author}</div>
              <div className="endpoint-meta">{p.text}</div>
              {p.createdAt && <div className="endpoint-meta muted">{p.createdAt}</div>}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
