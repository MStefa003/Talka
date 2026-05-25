import { useState, useEffect, useCallback } from "react";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";
import { useAppState, useConfig } from "../hooks/useAppState";
import type { AppConfig } from "../types";

const LANGUAGES = [
  { code: "auto", label: "Auto-detect" },
  { code: "en", label: "English" },
  { code: "el", label: "Greek" },
  { code: "es", label: "Spanish" },
  { code: "ru", label: "Russian" },
  { code: "ar", label: "Arabic" },
];

const STATUS_COLORS: Record<string, { dot: string; text: string }> = {
  idle:          { dot: "#34C759", text: "#4a8a55" },
  recording:     { dot: "#FF3B30", text: "#cc3030" },
  transcribing:  { dot: "#FF9500", text: "#cc7a00" },
  loading_model: { dot: "#007AFF", text: "#0060cc" },
};

export default function SettingsPage() {
  const { status, lastText, error } = useAppState();
  const { config, models, downloadProgress, saveConfig, downloadModel } = useConfig();
  const [local, setLocal] = useState<AppConfig | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<{ version: string } | null>(null);
  const [updating, setUpdating] = useState(false);

  // Silently check for updates on startup
  useEffect(() => {
    checkUpdate()
      .then((u) => { if (u) setUpdateInfo({ version: u.version }); })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (config && !local) setLocal(config);
  }, [config, local]);

  const save = useCallback(
    <K extends keyof AppConfig>(k: K, v: AppConfig[K]) => {
      if (!local) return;
      const next = { ...local, [k]: v };
      setLocal(next);
      saveConfig(next);
    },
    [local, saveConfig]
  );

  const installUpdate = useCallback(async () => {
    try {
      setUpdating(true);
      const u = await checkUpdate();
      if (u) await u.downloadAndInstall();
    } catch {
      setUpdating(false);
    }
  }, []);

  if (!local) {
    return (
      <div style={s.root}>
        <span style={{ color: "#2a2a2a", fontSize: 13 }}>loading...</span>
      </div>
    );
  }

  const anyModelReady = models.some((m) => m.downloaded);
  const isDownloading = !!downloadProgress;
  const progress = downloadProgress?.progress ?? 0;

  const statusColors = STATUS_COLORS[status] ?? STATUS_COLORS.idle;
  const statusLabel: Record<string, string> = {
    idle: anyModelReady ? "ready" : "setting up...",
    recording: "recording",
    transcribing: "transcribing",
    loading_model: "loading model",
  };

  return (
    <div style={s.root}>
      <div style={s.header}>
        <img src="/logo.png" alt="Talka" style={s.logo} />
        <div style={s.statusRow}>
          <span
            style={{
              ...s.statusDot,
              background: statusColors.dot,
              boxShadow: status !== "idle" ? `0 0 8px ${statusColors.dot}` : "none",
            }}
          />
          <span style={{ ...s.statusText, color: statusColors.text }}>
            {statusLabel[status] ?? status}
          </span>
        </div>
        {isDownloading && (
          <div style={s.progressWrap}>
            <div style={s.progressBar}>
              <div style={{ ...s.progressFill, width: `${progress}%` }} />
            </div>
            <span style={s.progressLabel}>Downloading model... {progress}%</span>
          </div>
        )}
        {!anyModelReady && !isDownloading && (
          <div style={{ ...s.progressWrap, textAlign: "center" as const }}>
            <span style={{ color: "#888", fontSize: 12, display: "block", marginBottom: 8 }}>
              Speech model not installed
            </span>
            <button
              onClick={() => local && downloadModel(local.model)}
              style={{ ...s.keyChip, background: "#FF3B30", color: "#fff", borderColor: "#cc2a20", fontSize: 12 }}
            >
              Download now (~150 MB)
            </button>
          </div>
        )}
      </div>

      <div style={s.section}>
        <SettingRow
          label="Push to Talk"
          sub={local.mode === "push_to_talk" ? "Hold key, release to transcribe" : "Tap once to start and stop"}
        >
          <Toggle
            on={local.mode === "push_to_talk"}
            onChange={(v) => save("mode", v ? "push_to_talk" : "toggle")}
          />
        </SettingRow>

        <HR />

        <SettingRow label="Shortcut" sub="Global keyboard trigger">
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <button
              onClick={() => setCapturing(true)}
              style={s.keyChip}
              onMouseEnter={(e) => (e.currentTarget.style.background = "#222")}
              onMouseLeave={(e) => (e.currentTarget.style.background = "#181818")}
            >
              {local.hotkey}
            </button>
            <button
              title="Reset to F9"
              onClick={() => save("hotkey", "F9")}
              style={s.iconBtn as React.CSSProperties}
              onMouseEnter={(e) => (e.currentTarget.style.color = "#bbb")}
              onMouseLeave={(e) => (e.currentTarget.style.color = "#555")}
            >
              &#8635;
            </button>
          </div>
        </SettingRow>

        <HR />

        <SettingRow label="Language" sub="Set explicitly for better accuracy">
          <select
            value={local.language ?? "auto"}
            onChange={(e) =>
              save("language", e.target.value === "auto" ? null : e.target.value)
            }
            style={s.select as React.CSSProperties}
          >
            {LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>{l.label}</option>
            ))}
          </select>
        </SettingRow>

        <HR />

        <SettingRow
          label="Model"
          sub={models.find((m) => m.name === local.model)?.downloaded ? "Loaded in memory" : "Will download on first use"}
        >
          <select
            value={local.model}
            onChange={(e) => {
              const name = e.target.value;
              save("model", name);
              const info = models.find((m) => m.name === name);
              if (info && !info.downloaded) downloadModel(name);
            }}
            style={s.select as React.CSSProperties}
          >
            {models.map((m) => (
              <option key={m.name} value={m.name}>
                {m.label.split(" — ")[0]}{!m.downloaded ? " ↓" : ""}
              </option>
            ))}
          </select>
        </SettingRow>

        <HR />

        <SettingRow
          label="Updates"
          sub={updateInfo ? `v${updateInfo.version} available` : "Up to date"}
        >
          <button
            onClick={installUpdate}
            disabled={!updateInfo || updating}
            style={{
              ...s.keyChip,
              background: updateInfo ? "#FF3B30" : "#181818",
              color: updateInfo ? "#fff" : "#444",
              borderColor: updateInfo ? "#cc2a20" : "#2a2a2a",
              cursor: updateInfo && !updating ? "pointer" : "default",
            } as React.CSSProperties}
          >
            {updating ? "installing…" : updateInfo ? "Install" : "✓ latest"}
          </button>
        </SettingRow>
      </div>

      {lastText && (
        <div style={s.transcript}>
          <span style={s.transcriptLabel}>last transcript</span>
          <p style={s.transcriptText}>{lastText}</p>
        </div>
      )}

      {error && (
        <div style={s.errorBox}>
          <p style={s.errorText}>{error}</p>
        </div>
      )}

      {capturing && (
        <KeyCaptureModal
          onSave={(k) => { save("hotkey", k); setCapturing(false); }}
          onClose={() => setCapturing(false)}
        />
      )}
    </div>
  );
}

function SettingRow({ label, sub, children }: { label: string; sub: string; children: React.ReactNode }) {
  return (
    <div style={s.row}>
      <div style={s.rowText}>
        <span style={s.rowLabel}>{label}</span>
        <span style={s.rowSub}>{sub}</span>
      </div>
      {children}
    </div>
  );
}

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      onClick={() => onChange(!on)}
      style={{
        flexShrink: 0,
        position: "relative",
        width: 44,
        height: 26,
        borderRadius: 13,
        border: "none",
        cursor: "pointer",
        background: on ? "#FF3B30" : "#2a2a2a",
        transition: "background 0.2s",
        padding: 0,
        outline: "none",
      }}
    >
      <span
        style={{
          position: "absolute",
          top: 3,
          left: on ? 21 : 3,
          width: 20,
          height: 20,
          borderRadius: "50%",
          background: "#fff",
          transition: "left 0.18s cubic-bezier(.4,0,.2,1)",
          boxShadow: "0 1px 3px rgba(0,0,0,0.4)",
        }}
      />
    </button>
  );
}

function HR() {
  return <div style={{ height: 1, background: "#1c1c1c", margin: "0 16px" }} />;
}

function KeyCaptureModal({ onSave, onClose }: { onSave: (k: string) => void; onClose: () => void }) {
  const [display, setDisplay] = useState<string[]>([]);
  const [done, setDone] = useState<string | null>(null);

  useEffect(() => {
    const MOD_MAP: Record<string, string> = {
      Control: "Ctrl", Alt: "Alt", Shift: "Shift", Meta: "Super",
    };
    const held = new Set<string>();

    const onDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Escape") { onClose(); return; }

      held.add(e.key);
      const mods = (["Control", "Alt", "Shift", "Meta"] as const)
        .filter((k) => held.has(k))
        .map((k) => MOD_MAP[k]);

      if (!(e.key in MOD_MAP)) {
        let main: string;
        if (e.code.startsWith("Key")) main = e.code.slice(3).toUpperCase();
        else if (/^F\d+$/.test(e.code)) main = e.code;
        else if (e.key === " ") main = "Space";
        else if (e.key.length === 1) main = e.key.toUpperCase();
        else main = e.key;

        const combo = [...mods, main].join("+");
        setDone(combo);
        setDisplay([...mods, main]);
        setTimeout(() => onSave(combo), 400);
      } else {
        setDisplay(mods);
      }
    };

    const onUp = (e: KeyboardEvent) => {
      held.delete(e.key);
      if (held.size === 0 && !done) setDisplay([]);
    };

    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    return () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
    };
  }, [done, onSave, onClose]);

  return (
    <div
      style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", background: "rgba(0,0,0,0.82)", backdropFilter: "blur(10px)", zIndex: 50 }}
      onClick={onClose}
    >
      <div
        style={{ background: "#181818", border: "1px solid #262626", borderRadius: 16, padding: "28px 32px", width: 280, textAlign: "center", boxShadow: "0 24px 60px rgba(0,0,0,0.7)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <p style={{ fontSize: 11, fontWeight: 600, letterSpacing: "0.12em", textTransform: "uppercase", color: "#555", margin: "0 0 20px" }}>
          Press your shortcut
        </p>
        <div style={{ minHeight: 52, display: "flex", alignItems: "center", justifyContent: "center", flexWrap: "wrap", gap: 6, marginBottom: 20 }}>
          {display.length > 0 ? (
            display.map((k) => (
              <span
                key={k}
                style={{ fontFamily: "monospace", fontSize: 14, fontWeight: 700, padding: "6px 12px", borderRadius: 8, background: done ? "#0f2010" : "#1e1e1e", border: `1px solid ${done ? "#2a4a2a" : "#2e2e2e"}`, color: done ? "#5aba5a" : "#d0d0d0", transition: "all 0.15s" }}
              >
                {k}
              </span>
            ))
          ) : (
            <span style={{ color: "#2e2e2e", fontSize: 13 }}>waiting...</span>
          )}
        </div>
        <p style={{ fontSize: 11, color: "#333", margin: 0 }}>Escape to cancel</p>
      </div>
    </div>
  );
}

const s = {
  root: { height: "100%", background: "#111", display: "flex", flexDirection: "column" as const, color: "#e0e0e0", fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif", overflow: "hidden", userSelect: "none" as const },
  header: { display: "flex", flexDirection: "column" as const, alignItems: "center", padding: "28px 20px 20px" },
  logo: { height: 80, width: "auto", marginBottom: 12 },
  statusRow: { display: "flex", alignItems: "center", gap: 6 },
  statusDot: { width: 7, height: 7, borderRadius: "50%", flexShrink: 0, transition: "background 0.3s, box-shadow 0.3s" },
  statusText: { fontSize: 12, fontWeight: 500, transition: "color 0.3s" },
  progressWrap: { marginTop: 14, width: "100%", maxWidth: 220 },
  progressBar: { height: 2, background: "#1e1e1e", borderRadius: 2, overflow: "hidden", marginBottom: 5 },
  progressFill: { height: "100%", background: "#FF3B30", borderRadius: 2, transition: "width 0.4s" },
  progressLabel: { display: "block", fontSize: 11, color: "#444", textAlign: "center" as const },
  section: { margin: "0 16px", borderRadius: 12, border: "1px solid #1c1c1c", background: "#161616", overflow: "hidden" },
  row: { display: "flex", alignItems: "center", justifyContent: "space-between", padding: "13px 16px", gap: 12 },
  rowText: { display: "flex", flexDirection: "column" as const, gap: 2, flex: 1, minWidth: 0 },
  rowLabel: { fontSize: 13, fontWeight: 500, color: "#e0e0e0" },
  rowSub: { fontSize: 11, color: "#555" },
  keyChip: { fontFamily: "monospace", fontSize: 12, fontWeight: 700, padding: "4px 10px", borderRadius: 6, background: "#181818", border: "1px solid #2a2a2a", color: "#c8c8c8", cursor: "pointer", outline: "none", transition: "background 0.15s", letterSpacing: "0.03em", flexShrink: 0 },
  iconBtn: { width: 28, height: 28, borderRadius: 6, background: "#181818", border: "1px solid #2a2a2a", color: "#555", cursor: "pointer", fontSize: 15, display: "flex", alignItems: "center", justifyContent: "center", outline: "none", transition: "color 0.15s", padding: 0, flexShrink: 0 },
  select: { background: "#181818", border: "1px solid #2a2a2a", color: "#c8c8c8", borderRadius: 6, padding: "4px 8px", fontSize: 12, cursor: "pointer", outline: "none", flexShrink: 0 },
  transcript: { margin: "12px 16px 0", padding: "10px 14px", borderRadius: 10, background: "#151515", border: "1px solid #1c1c1c" },
  transcriptLabel: { display: "block", fontSize: 9, fontWeight: 700, letterSpacing: "0.1em", textTransform: "uppercase" as const, color: "#333", marginBottom: 5 },
  transcriptText: { fontSize: 13, color: "#bbb", lineHeight: 1.5, margin: 0, whiteSpace: "pre-wrap" as const },
  errorBox: { margin: "8px 16px 0", padding: "8px 14px", borderRadius: 8, background: "#1a0d0d", border: "1px solid #3a1818" },
  errorText: { fontSize: 12, color: "#f06060", margin: 0 },
} as const;
