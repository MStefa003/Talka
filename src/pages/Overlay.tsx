import { useAppState } from "../hooks/useAppState";
import type { AppStatus } from "../types";

const STATUS_CONFIG: Record<
  AppStatus,
  { label: string; color: string; pulse: boolean }
> = {
  idle: { label: "", color: "", pulse: false },
  recording: { label: "Recording", color: "#ff4d4d", pulse: true },
  transcribing: { label: "Processing…", color: "#ffaa00", pulse: false },
  loading_model: { label: "Loading model…", color: "#6c63ff", pulse: false },
};

export default function OverlayPage() {
  const { status } = useAppState();
  const cfg = STATUS_CONFIG[status];

  if (status === "idle") {
    return <div className="w-full h-full" />;
  }

  return (
    <div
      className="flex items-center gap-2 px-4 py-2 rounded-full text-white text-sm font-semibold select-none"
      style={{
        background: "rgba(15,15,19,0.92)",
        backdropFilter: "blur(8px)",
        border: `1px solid ${cfg.color}44`,
        boxShadow: `0 0 12px ${cfg.color}33`,
      }}
    >
      <span
        className={cfg.pulse ? "animate-pulse" : ""}
        style={{
          width: 8,
          height: 8,
          borderRadius: "50%",
          background: cfg.color,
          display: "inline-block",
          flexShrink: 0,
        }}
      />
      <span style={{ color: cfg.color }}>{cfg.label}</span>
    </div>
  );
}
