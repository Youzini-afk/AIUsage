import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  BarChart3,
  Bell,
  Bot,
  Braces,
  Gauge,
  KeyRound,
  Layers3,
  MessageSquareText,
  MonitorCog,
  Network,
  Settings,
  TerminalSquare
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

type FeatureStatus = "planned" | "foundationReady" | "inProgress" | "complete" | "blocked";

type ProductSurface = {
  id: string;
  label: string;
  status: FeatureStatus;
};

type DesktopSnapshot = {
  appName: string;
  phase: string;
  surfaces: ProductSurface[];
  providers: ProductSurface[];
  releaseTargets: string[];
};

const fallbackSnapshot: DesktopSnapshot = {
  appName: "AIUsage",
  phase: "Windows Phase A",
  surfaces: [
    { id: "dashboard", label: "Dashboard", status: "foundationReady" },
    { id: "subscriptions", label: "Subscriptions", status: "foundationReady" },
    { id: "apiProviders", label: "API Providers", status: "foundationReady" },
    { id: "codexProxy", label: "Codex Proxy", status: "foundationReady" },
    { id: "opencodeProxy", label: "OpenCode Proxy", status: "foundationReady" },
    { id: "claudeProxy", label: "Claude Code Proxy", status: "foundationReady" },
    { id: "usageStats", label: "Usage Stats", status: "foundationReady" },
    { id: "callAnalytics", label: "Call Analytics", status: "foundationReady" },
    { id: "inbox", label: "Inbox", status: "foundationReady" },
    { id: "settings", label: "Settings", status: "foundationReady" }
  ],
  providers: [
    { id: "codex", label: "Codex", status: "foundationReady" },
    { id: "copilot", label: "Copilot", status: "foundationReady" },
    { id: "cursor", label: "Cursor", status: "foundationReady" },
    { id: "gemini", label: "Gemini CLI", status: "foundationReady" },
    { id: "opencode", label: "OpenCode", status: "foundationReady" }
  ],
  releaseTargets: ["NSIS setup.exe", "MSI", "Tauri updater"]
};

const sections = [
  { id: "dashboard", label: "Dashboard", icon: Gauge },
  { id: "subscriptions", label: "Subscriptions", icon: KeyRound },
  { id: "apiProviders", label: "API Providers", icon: Layers3 },
  { id: "codexProxy", label: "Codex Proxy", icon: TerminalSquare },
  { id: "opencodeProxy", label: "OpenCode Proxy", icon: Braces },
  { id: "claudeProxy", label: "Claude Code Proxy", icon: Bot },
  { id: "usageStats", label: "Usage Stats", icon: BarChart3 },
  { id: "callAnalytics", label: "Call Analytics", icon: Activity },
  { id: "inbox", label: "Inbox", icon: MessageSquareText },
  { id: "settings", label: "Settings", icon: Settings }
];

function statusLabel(status: FeatureStatus): string {
  switch (status) {
    case "foundationReady":
      return "Foundation";
    case "inProgress":
      return "In progress";
    case "complete":
      return "Complete";
    case "blocked":
      return "Blocked";
    default:
      return "Planned";
  }
}

export function App() {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>(fallbackSnapshot);
  const [activeSection, setActiveSection] = useState("dashboard");

  useEffect(() => {
    invoke<DesktopSnapshot>("app_snapshot")
      .then(setSnapshot)
      .catch(() => setSnapshot(fallbackSnapshot));
  }, []);

  const activeSurface = useMemo(
    () => snapshot.surfaces.find((surface) => surface.id === activeSection),
    [activeSection, snapshot.surfaces]
  );

  const readyCount = snapshot.surfaces.filter((surface) => surface.status !== "planned").length;

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-row">
          <div className="brand-mark">AI</div>
          <div>
            <h1>{snapshot.appName}</h1>
            <p>{snapshot.phase}</p>
          </div>
        </div>
        <nav aria-label="AIUsage sections">
          {sections.map((section) => {
            const Icon = section.icon;
            const isActive = activeSection === section.id;
            return (
              <button
                key={section.id}
                className={isActive ? "nav-item active" : "nav-item"}
                type="button"
                onClick={() => setActiveSection(section.id)}
              >
                <Icon size={17} />
                <span>{section.label}</span>
              </button>
            );
          })}
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">Windows product line</p>
            <h2>{activeSurface?.label ?? "Dashboard"}</h2>
          </div>
          <div className="topbar-actions">
            <button type="button" title="Notifications" aria-label="Notifications">
              <Bell size={18} />
            </button>
            <button type="button" title="Platform settings" aria-label="Platform settings">
              <MonitorCog size={18} />
            </button>
          </div>
        </header>

        <div className="summary-grid">
          <section className="stat-tile">
            <span>Product surfaces</span>
            <strong>
              {readyCount}/{snapshot.surfaces.length}
            </strong>
          </section>
          <section className="stat-tile">
            <span>Provider contracts</span>
            <strong>{snapshot.providers.length}</strong>
          </section>
          <section className="stat-tile">
            <span>Release targets</span>
            <strong>{snapshot.releaseTargets.length}</strong>
          </section>
        </div>

        <section className="panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Surface readiness</p>
              <h3>Windows parity map</h3>
            </div>
            <Network size={18} />
          </div>
          <div className="surface-list">
            {snapshot.surfaces.map((surface) => (
              <article key={surface.id} className="surface-row">
                <div>
                  <strong>{surface.label}</strong>
                  <span>{surface.id}</span>
                </div>
                <span className={`status ${surface.status}`}>{statusLabel(surface.status)}</span>
              </article>
            ))}
          </div>
        </section>

        <section className="panel compact-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Release foundation</p>
              <h3>Windows artifacts</h3>
            </div>
          </div>
          <div className="target-row">
            {snapshot.releaseTargets.map((target) => (
              <span key={target}>{target}</span>
            ))}
          </div>
        </section>
      </section>
    </main>
  );
}
