import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import {
  Activity,
  BarChart3,
  Bell,
  Bot,
  Braces,
  Download,
  Gauge,
  KeyRound,
  Layers3,
  MessageSquareText,
  MonitorCog,
  Network,
  RefreshCw,
  Settings,
  ServerCog,
  TerminalSquare
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

type FeatureStatus = "planned" | "foundationReady" | "inProgress" | "complete" | "blocked";

type ProductSurface = {
  id: string;
  label: string;
  status: FeatureStatus;
};

type ProxyTrack = "claudeCode" | "codex" | "openCode" | "global";

type DesktopSnapshot = {
  appName: string;
  phase: string;
  surfaces: ProductSurface[];
  providers: ProductSurface[];
  releaseTargets: string[];
};

type DesktopPathSnapshot = {
  appConfigDir: string | null;
  appDataDir: string | null;
  codexHome: string | null;
  claudeHome: string | null;
  opencodeConfigDir: string | null;
};

type SystemProxySummary = {
  http: string | null;
  https: string | null;
  socks: string | null;
  anyEnabled: boolean;
  errorMessage: string | null;
};

type BrowserProfileSummary = {
  browserName: string;
  profileName: string;
  cookiesDbPath: string;
};

type ProxyPortOwnerSummary = {
  port: number;
  processId: number;
  imagePath: string | null;
};

type ProxyPortPreflight = {
  track: ProxyTrack;
  bindHost: string;
  port: number;
  available: boolean;
  owner: ProxyPortOwnerSummary | null;
  errorMessage: string | null;
};

type PlatformEnvironmentSnapshot = {
  generatedAtEpochMs: number;
  paths: DesktopPathSnapshot;
  systemProxy: SystemProxySummary;
  browserProfiles: BrowserProfileSummary[];
  browserProfileError: string | null;
  defaultProxyPorts: ProxyPortPreflight[];
};

type ProxyRuntimeState = "stopped" | "starting" | "running" | "stopping" | "failed";

type ProxyHealth = {
  track: ProxyTrack;
  state: ProxyRuntimeState;
  listeningPort: number | null;
};

type ProxyUsageArchiveSummary = {
  track: ProxyHealth["track"];
  path: string;
  records: number;
  updatedAtEpochMs: number | null;
};

type TokenTotals = {
  requests: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
};

type ProxyUsageStats = {
  totals: TokenTotals;
  byTrack: Array<{ track: ProxyHealth["track"]; totals: TokenTotals }>;
  byModel: Array<{ track: ProxyHealth["track"]; model: string; totals: TokenTotals }>;
  updatedAtEpochMs: number | null;
};

type CallAnalyticsSource = "claude" | "codex" | "openCode";

type CallAnalyticsPathStatus = {
  path: string;
  exists: boolean;
};

type CallAnalyticsInventorySourceStatus = {
  source: CallAnalyticsSource;
  available: boolean;
  configPaths: CallAnalyticsPathStatus[];
  sessionPaths: CallAnalyticsPathStatus[];
  skillPaths: CallAnalyticsPathStatus[];
  configFileCount: number;
  sessionFileCount: number;
  skillCount: number;
  mcpServerCount: number;
  skillNames: string[];
  mcpServerNames: string[];
  warnings: string[];
};

type CallAnalyticsInventorySnapshot = {
  generatedAtEpochMs: number;
  sources: CallAnalyticsInventorySourceStatus[];
};

type CallAnalyticsKind = "mcp" | "skill" | "builtin" | "webSearch" | "other";

type CallAnalyticsEntry = {
  source: CallAnalyticsSource;
  kind: CallAnalyticsKind;
  name: string;
  server: string | null;
  agent: string | null;
  dayKey: string;
  count: number;
  outcomeKnownCount: number;
  successCount: number;
  durationSampleCount: number;
  durationMsTotal: number;
};

type CallAnalyticsSourceScanStatus = {
  source: CallAnalyticsSource;
  available: boolean;
  eventCount: number;
  filesScanned: number;
  errorCode: string | null;
  warnings: string[];
};

type CallAnalyticsSnapshot = {
  generatedAtEpochMs: number;
  rangeKey: string;
  entries: CallAnalyticsEntry[];
  installedSkills: Array<{ source: CallAnalyticsSource; name: string }>;
  installedMcpServers: Array<{ source: CallAnalyticsSource; name: string }>;
  agentInvocations: Array<{ source: CallAnalyticsSource; agent: string; dayKey: string; count: number }>;
  sources: CallAnalyticsSourceScanStatus[];
};

type ThemeMode = "system" | "light" | "dark";
type AppLanguage = "en" | "zh";

type AppSettingsDocument = {
  version: number;
  themeMode: ThemeMode;
  language: AppLanguage;
  autoRefreshIntervalSecs: number;
  proxyAutoRestoreOnLaunch: boolean;
  minimizeToTrayOnClose: boolean;
  keepRunningInBackground: boolean;
  launchAtLogin: boolean;
};

type AppSettingsSnapshot = {
  settings: AppSettingsDocument;
  settingsPath: string;
  autostartEnabled: boolean;
  autostartError: string | null;
};

type DiagnosticsPathSummary = {
  label: string;
  path: string;
  exists: boolean;
  fileCount: number;
  totalBytes: number;
};

type DiagnosticsFileSummary = {
  label: string;
  path: string;
  bytes: number;
  modifiedAtEpochMs: number | null;
};

type DiagnosticsExportSnapshot = {
  version: number;
  generatedAtEpochMs: number;
  exportPath: string;
  paths: DiagnosticsPathSummary[];
  recentFiles: DiagnosticsFileSummary[];
  warningMessages: string[];
};

type CredentialSummary = {
  id: string;
  providerId: string;
  label: string;
  kind: string;
  hasSecret: boolean;
  metadata: unknown;
  updatedAtEpochMs: number;
};

type UpdaterState = "idle" | "checking" | "current" | "available" | "downloading" | "installing" | "installed" | "error";

type UpdaterProgress = {
  downloadedBytes: number;
  contentLength: number | null;
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

const sectionIds = new Set(sections.map((section) => section.id));

const defaultAppSettings: AppSettingsDocument = {
  version: 1,
  themeMode: "system",
  language: "en",
  autoRefreshIntervalSecs: 300,
  proxyAutoRestoreOnLaunch: false,
  minimizeToTrayOnClose: true,
  keepRunningInBackground: true,
  launchAtLogin: false
};

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

function runtimeStateLabel(state: ProxyRuntimeState): string {
  switch (state) {
    case "running":
      return "Running";
    case "starting":
      return "Starting";
    case "stopping":
      return "Stopping";
    case "failed":
      return "Failed";
    default:
      return "Stopped";
  }
}

function proxyTrackLabel(track: ProxyHealth["track"]): string {
  switch (track) {
    case "claudeCode":
      return "Claude Code";
    case "openCode":
      return "OpenCode";
    case "global":
      return "Global";
    default:
      return "Codex";
  }
}

function callSourceLabel(source: CallAnalyticsSource): string {
  switch (source) {
    case "claude":
      return "Claude Code";
    case "openCode":
      return "OpenCode";
    default:
      return "Codex";
  }
}

function callKindLabel(kind: CallAnalyticsKind): string {
  switch (kind) {
    case "mcp":
      return "MCP";
    case "skill":
      return "Skill";
    case "webSearch":
      return "Web";
    case "builtin":
      return "Tool";
    default:
      return "Other";
  }
}

function updaterStateLabel(state: UpdaterState): string {
  switch (state) {
    case "checking":
      return "Checking";
    case "current":
      return "Up to date";
    case "available":
      return "Update available";
    case "downloading":
      return "Downloading";
    case "installing":
      return "Installing";
    case "installed":
      return "Installer started";
    case "error":
      return "Unavailable";
    default:
      return "Ready";
  }
}

function updaterStatusClass(state: UpdaterState): string {
  switch (state) {
    case "current":
    case "installed":
      return "complete";
    case "available":
    case "checking":
    case "downloading":
    case "installing":
      return "inProgress";
    case "error":
      return "blocked";
    default:
      return "planned";
  }
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

function formatUpdaterError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("invoke") && message.includes("undefined")) {
    return "Updater is available in the packaged Windows app.";
  }
  return message;
}

export function App() {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>(fallbackSnapshot);
  const [platformEnvironment, setPlatformEnvironment] = useState<PlatformEnvironmentSnapshot | null>(null);
  const [proxyHealth, setProxyHealth] = useState<ProxyHealth[]>([]);
  const [proxyArchives, setProxyArchives] = useState<ProxyUsageArchiveSummary[]>([]);
  const [proxyUsageStats, setProxyUsageStats] = useState<ProxyUsageStats | null>(null);
  const [callInventory, setCallInventory] = useState<CallAnalyticsInventorySnapshot | null>(null);
  const [callSnapshot, setCallSnapshot] = useState<CallAnalyticsSnapshot | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettingsSnapshot | null>(null);
  const [diagnosticsExport, setDiagnosticsExport] = useState<DiagnosticsExportSnapshot | null>(null);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);
  const [diagnosticsExporting, setDiagnosticsExporting] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<Update | null>(null);
  const [updaterState, setUpdaterState] = useState<UpdaterState>("idle");
  const [updaterError, setUpdaterError] = useState<string | null>(null);
  const [updaterProgress, setUpdaterProgress] = useState<UpdaterProgress | null>(null);
  const [credentials, setCredentials] = useState<CredentialSummary[]>([]);
  const [activeSection, setActiveSection] = useState("dashboard");

  useEffect(() => {
    invoke<DesktopSnapshot>("app_snapshot")
      .then(setSnapshot)
      .catch(() => setSnapshot(fallbackSnapshot));
    invoke<PlatformEnvironmentSnapshot>("platform_environment")
      .then(setPlatformEnvironment)
      .catch(() => setPlatformEnvironment(null));
    invoke<ProxyHealth[]>("proxy_statuses")
      .then(setProxyHealth)
      .catch(() => setProxyHealth([]));
    invoke<ProxyUsageArchiveSummary[]>("proxy_usage_archives")
      .then(setProxyArchives)
      .catch(() => setProxyArchives([]));
    invoke<ProxyUsageStats>("proxy_usage_stats")
      .then(setProxyUsageStats)
      .catch(() => setProxyUsageStats(null));
    invoke<CallAnalyticsInventorySnapshot>("call_analytics_inventory")
      .then(setCallInventory)
      .catch(() => setCallInventory(null));
    invoke<CallAnalyticsSnapshot>("call_analytics_snapshot")
      .then(setCallSnapshot)
      .catch(() => setCallSnapshot(null));
    invoke<AppSettingsSnapshot>("app_settings")
      .then(setAppSettings)
      .catch(() => setAppSettings(null));
    invoke<CredentialSummary[]>("credentials")
      .then(setCredentials)
      .catch(() => setCredentials([]));
  }, []);

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined;

    listen<string>("aiusage-open-section", (event) => {
      if (sectionIds.has(event.payload)) {
        setActiveSection(event.payload);
      }
    }).then((listener) => {
      if (mounted) {
        unlisten = listener;
      } else {
        listener();
      }
    });

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    return () => {
      pendingUpdate?.close().catch(() => undefined);
    };
  }, [pendingUpdate]);

  const activeSurface = useMemo(
    () => snapshot.surfaces.find((surface) => surface.id === activeSection),
    [activeSection, snapshot.surfaces]
  );

  const readyCount = snapshot.surfaces.filter((surface) => surface.status !== "planned").length;
  const runningProxyCount = proxyHealth.filter((health) => health.state === "running").length;
  const archivedUsageRows = proxyArchives.reduce((total, archive) => total + archive.records, 0);
  const totalProxyTokens = proxyUsageStats
    ? proxyUsageStats.totals.inputTokens +
      proxyUsageStats.totals.outputTokens +
      proxyUsageStats.totals.cacheReadTokens +
      proxyUsageStats.totals.cacheWriteTokens
    : 0;
  const inventoryRows =
    callInventory?.sources ??
    ([
      {
        source: "claude",
        available: false,
        configPaths: [],
        sessionPaths: [],
        skillPaths: [],
        configFileCount: 0,
        sessionFileCount: 0,
        skillCount: 0,
        mcpServerCount: 0,
        skillNames: [],
        mcpServerNames: [],
        warnings: []
      },
      {
        source: "codex",
        available: false,
        configPaths: [],
        sessionPaths: [],
        skillPaths: [],
        configFileCount: 0,
        sessionFileCount: 0,
        skillCount: 0,
        mcpServerCount: 0,
        skillNames: [],
        mcpServerNames: [],
        warnings: []
      },
      {
        source: "openCode",
        available: false,
        configPaths: [],
        sessionPaths: [],
        skillPaths: [],
        configFileCount: 0,
        sessionFileCount: 0,
        skillCount: 0,
        mcpServerCount: 0,
        skillNames: [],
        mcpServerNames: [],
        warnings: []
      }
    ] satisfies CallAnalyticsInventorySourceStatus[]);
  const availableCallSources = inventoryRows.filter((row) => row.available).length;
  const detectedSkills = inventoryRows.reduce((total, row) => total + row.skillCount, 0);
  const detectedMcpServers = inventoryRows.reduce((total, row) => total + row.mcpServerCount, 0);
  const callEntries = callSnapshot?.entries ?? [];
  const totalCallEvents = callEntries.reduce((total, entry) => total + entry.count, 0);
  const mcpCallEvents = callEntries
    .filter((entry) => entry.kind === "mcp")
    .reduce((total, entry) => total + entry.count, 0);
  const skillCallEvents = callEntries
    .filter((entry) => entry.kind === "skill")
    .reduce((total, entry) => total + entry.count, 0);
  const toolCallEvents = callEntries
    .filter((entry) => entry.kind === "builtin" || entry.kind === "webSearch" || entry.kind === "other")
    .reduce((total, entry) => total + entry.count, 0);
  const topCallEntries = [...callEntries]
    .sort((left, right) => right.count - left.count || left.name.localeCompare(right.name))
    .slice(0, 6);
  const settingsDocument = appSettings?.settings ?? defaultAppSettings;
  const platformPathRows = [
    { label: "App config", path: platformEnvironment?.paths.appConfigDir ?? "%APPDATA%\\AIUsage" },
    { label: "App data", path: platformEnvironment?.paths.appDataDir ?? "%LOCALAPPDATA%\\AIUsage" },
    { label: "Claude", path: platformEnvironment?.paths.claudeHome ?? "%USERPROFILE%\\.claude" },
    { label: "Codex", path: platformEnvironment?.paths.codexHome ?? "%USERPROFILE%\\.codex" },
    {
      label: "OpenCode",
      path: platformEnvironment?.paths.opencodeConfigDir ?? "%USERPROFILE%\\.config\\opencode"
    }
  ];
  const systemProxy = platformEnvironment?.systemProxy;
  const browserProfiles = platformEnvironment?.browserProfiles ?? [];
  const defaultProxyPorts =
    platformEnvironment?.defaultProxyPorts ??
    ([
      { track: "codex", bindHost: "127.0.0.1", port: 14399, available: true, owner: null, errorMessage: null },
      { track: "claudeCode", bindHost: "127.0.0.1", port: 14400, available: true, owner: null, errorMessage: null },
      { track: "openCode", bindHost: "127.0.0.1", port: 14401, available: true, owner: null, errorMessage: null },
      { track: "global", bindHost: "127.0.0.1", port: 14402, available: true, owner: null, errorMessage: null }
    ] satisfies ProxyPortPreflight[]);
  const availablePlatformPaths = platformPathRows.filter((row) => !row.path.includes("%")).length;
  const updaterBusy = updaterState === "checking" || updaterState === "downloading" || updaterState === "installing";
  const updaterProgressPercent =
    updaterProgress?.contentLength && updaterProgress.contentLength > 0
      ? Math.min(100, Math.round((updaterProgress.downloadedBytes / updaterProgress.contentLength) * 100))
      : null;
  const updaterDetail = pendingUpdate
    ? `Current ${pendingUpdate.currentVersion} · Available ${pendingUpdate.version}`
    : updaterState === "current"
      ? "Current installed build"
      : "Signed Windows updater endpoint";

  function saveSettingsPatch(patch: Partial<AppSettingsDocument>) {
    const previous = appSettings;
    const nextSettings = { ...settingsDocument, ...patch };
    setAppSettings({
      settings: nextSettings,
      settingsPath: previous?.settingsPath ?? "",
      autostartEnabled: nextSettings.launchAtLogin,
      autostartError: null
    });
    invoke<AppSettingsSnapshot>("save_app_settings", { settings: nextSettings })
      .then(setAppSettings)
      .catch(() => setAppSettings(previous));
  }

  function exportDiagnostics() {
    setDiagnosticsExporting(true);
    setDiagnosticsError(null);
    invoke<DiagnosticsExportSnapshot>("export_diagnostics")
      .then(setDiagnosticsExport)
      .catch((error) => setDiagnosticsError(String(error)))
      .finally(() => setDiagnosticsExporting(false));
  }

  async function checkForUpdates() {
    setUpdaterState("checking");
    setUpdaterError(null);
    setUpdaterProgress(null);
    setPendingUpdate(null);

    try {
      const update = await check({ timeout: 30000 });
      setPendingUpdate(update);
      setUpdaterState(update ? "available" : "current");
    } catch (error) {
      setPendingUpdate(null);
      setUpdaterState("error");
      setUpdaterError(formatUpdaterError(error));
    }
  }

  async function installPendingUpdate() {
    if (!pendingUpdate) {
      return;
    }

    let downloadedBytes = 0;
    let contentLength: number | null = null;
    setUpdaterState("downloading");
    setUpdaterError(null);
    setUpdaterProgress({ downloadedBytes, contentLength });

    try {
      await pendingUpdate.downloadAndInstall((event: DownloadEvent) => {
        switch (event.event) {
          case "Started":
            downloadedBytes = 0;
            contentLength = event.data.contentLength ?? null;
            setUpdaterProgress({ downloadedBytes, contentLength });
            break;
          case "Progress":
            downloadedBytes += event.data.chunkLength;
            setUpdaterProgress({ downloadedBytes, contentLength });
            break;
          case "Finished":
            setUpdaterState("installing");
            break;
        }
      });
      setUpdaterState("installed");
      setPendingUpdate(null);
    } catch (error) {
      setUpdaterState("error");
      setUpdaterError(formatUpdaterError(error));
    }
  }

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
            <button
              type="button"
              title="Platform settings"
              aria-label="Platform settings"
              onClick={() => setActiveSection("settings")}
            >
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
          <section className="stat-tile">
            <span>Proxy tracks running</span>
            <strong>
              {runningProxyCount}/{proxyHealth.length || 4}
            </strong>
          </section>
          <section className="stat-tile">
            <span>Usage archive rows</span>
            <strong>{archivedUsageRows}</strong>
          </section>
          <section className="stat-tile">
            <span>Proxy tokens</span>
            <strong>{totalProxyTokens.toLocaleString()}</strong>
          </section>
          <section className="stat-tile">
            <span>Call sources</span>
            <strong>
              {availableCallSources}/{inventoryRows.length}
            </strong>
          </section>
          <section className="stat-tile">
            <span>Skills detected</span>
            <strong>{detectedSkills.toLocaleString()}</strong>
          </section>
          <section className="stat-tile">
            <span>MCP servers</span>
            <strong>{detectedMcpServers.toLocaleString()}</strong>
          </section>
          <section className="stat-tile">
            <span>Call events</span>
            <strong>{totalCallEvents.toLocaleString()}</strong>
          </section>
          <section className="stat-tile">
            <span>Stored credentials</span>
            <strong>{credentials.length}</strong>
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
              <p className="eyebrow">Platform status</p>
              <h3>Windows environment</h3>
            </div>
            <MonitorCog size={18} />
          </div>
          <div className="usage-grid">
            <section className="usage-meter">
              <span>System proxy</span>
              <strong>{systemProxy?.anyEnabled ? "On" : "Off"}</strong>
            </section>
            <section className="usage-meter">
              <span>Browser profiles</span>
              <strong>{browserProfiles.length}</strong>
            </section>
            <section className="usage-meter">
              <span>Known paths</span>
              <strong>
                {availablePlatformPaths}/{platformPathRows.length}
              </strong>
            </section>
            <section className="usage-meter">
              <span>Proxy endpoints</span>
              <strong>{[systemProxy?.http, systemProxy?.https, systemProxy?.socks].filter(Boolean).length}</strong>
            </section>
            <section className="usage-meter">
              <span>Free default ports</span>
              <strong>
                {defaultProxyPorts.filter((port) => port.available).length}/{defaultProxyPorts.length}
              </strong>
            </section>
          </div>
          <div className="surface-list single-column">
            {platformPathRows.map((row) => (
              <article key={row.label} className="surface-row">
                <div>
                  <strong>{row.label}</strong>
                  <span>{row.path}</span>
                </div>
              </article>
            ))}
            <article className="surface-row">
              <div>
                <strong>System proxy</strong>
                <span>{systemProxy?.http ?? systemProxy?.https ?? systemProxy?.socks ?? "Not configured"}</span>
              </div>
              <span className={`status ${systemProxy?.anyEnabled ? "runtime-running" : "runtime-stopped"}`}>
                {systemProxy?.anyEnabled ? "Enabled" : "Disabled"}
              </span>
            </article>
            {defaultProxyPorts.map((port) => (
              <article key={`${port.track}-${port.port}`} className="surface-row">
                <div>
                  <strong>
                    {proxyTrackLabel(port.track)} · {port.bindHost}:{port.port}
                  </strong>
                  <span>
                    {port.owner
                      ? `PID ${port.owner.processId}${port.owner.imagePath ? ` · ${port.owner.imagePath}` : ""}`
                      : port.errorMessage ?? "Available"}
                  </span>
                </div>
                <span className={`status ${port.available ? "runtime-running" : "runtime-failed"}`}>
                  {port.available ? "Free" : "Busy"}
                </span>
              </article>
            ))}
            {(browserProfiles.length
              ? browserProfiles.slice(0, 4)
              : [{ browserName: "Browser profile", profileName: "Not detected", cookiesDbPath: "" }]
            ).map((profile) => (
              <article key={`${profile.browserName}-${profile.profileName}`} className="surface-row">
                <div>
                  <strong>
                    {profile.browserName} · {profile.profileName}
                  </strong>
                  <span>{profile.cookiesDbPath || "No cookie database found"}</span>
                </div>
              </article>
            ))}
          </div>
          {systemProxy?.errorMessage ? <div className="settings-error">{systemProxy.errorMessage}</div> : null}
          {platformEnvironment?.browserProfileError ? (
            <div className="settings-error">{platformEnvironment.browserProfileError}</div>
          ) : null}
        </section>

        <section className="panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Local inventory</p>
              <h3>Call Analytics sources</h3>
            </div>
            <Activity size={18} />
          </div>
          <div className="surface-list">
            {inventoryRows.map((row) => {
              const stateClass = row.warnings.length ? "blocked" : row.available ? "complete" : "planned";
              const stateLabel = row.warnings.length ? "Warning" : row.available ? "Detected" : "No data";
              return (
                <article key={row.source} className="surface-row inventory-row">
                  <div>
                    <strong>{callSourceLabel(row.source)}</strong>
                    <span>
                      {row.mcpServerCount.toLocaleString()} MCP · {row.skillCount.toLocaleString()} skills ·{" "}
                      {row.sessionFileCount.toLocaleString()} session files
                    </span>
                    <span>
                      {row.configFileCount}/{row.configPaths.length || 1} config files ·{" "}
                      {row.warnings.length.toLocaleString()} warnings
                    </span>
                  </div>
                  <span className={`status ${stateClass}`}>{stateLabel}</span>
                </article>
              );
            })}
          </div>
        </section>

        <section className="panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Event aggregation</p>
              <h3>Call Analytics ledger</h3>
            </div>
            <Activity size={18} />
          </div>
          <div className="usage-grid">
            <section className="usage-meter">
              <span>Total</span>
              <strong>{totalCallEvents.toLocaleString()}</strong>
            </section>
            <section className="usage-meter">
              <span>MCP</span>
              <strong>{mcpCallEvents.toLocaleString()}</strong>
            </section>
            <section className="usage-meter">
              <span>Skills</span>
              <strong>{skillCallEvents.toLocaleString()}</strong>
            </section>
            <section className="usage-meter">
              <span>Tools</span>
              <strong>{toolCallEvents.toLocaleString()}</strong>
            </section>
          </div>
          <div className="surface-list usage-list">
            {(topCallEntries.length
              ? topCallEntries
              : [
                  {
                    source: "codex",
                    kind: "other",
                    name: "No calls indexed",
                    server: null,
                    agent: null,
                    dayKey: "",
                    count: 0,
                    outcomeKnownCount: 0,
                    successCount: 0,
                    durationSampleCount: 0,
                    durationMsTotal: 0
                  }
                ] satisfies CallAnalyticsEntry[]
            ).map((entry) => (
              <article key={`${entry.source}-${entry.kind}-${entry.name}-${entry.agent ?? "main"}`} className="surface-row">
                <div>
                  <strong>{entry.name}</strong>
                  <span>
                    {callSourceLabel(entry.source)} · {callKindLabel(entry.kind)}
                    {entry.agent ? ` · ${entry.agent}` : ""}
                  </span>
                </div>
                <span className="usage-total">{entry.count.toLocaleString()}</span>
              </article>
            ))}
          </div>
        </section>

        <section className="panel compact-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Preferences</p>
              <h3>Windows settings</h3>
            </div>
            <Settings size={18} />
          </div>
          <div className="settings-list">
            <label className="setting-row">
              <span>Theme</span>
              <select
                value={settingsDocument.themeMode}
                onChange={(event) => saveSettingsPatch({ themeMode: event.target.value as ThemeMode })}
              >
                <option value="system">System</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </label>
            <label className="setting-row">
              <span>Language</span>
              <select
                value={settingsDocument.language}
                onChange={(event) => saveSettingsPatch({ language: event.target.value as AppLanguage })}
              >
                <option value="en">English</option>
                <option value="zh">中文</option>
              </select>
            </label>
            <label className="setting-row">
              <span>Refresh interval</span>
              <select
                value={settingsDocument.autoRefreshIntervalSecs}
                onChange={(event) => saveSettingsPatch({ autoRefreshIntervalSecs: Number(event.target.value) })}
              >
                <option value={30}>30s</option>
                <option value={60}>1m</option>
                <option value={300}>5m</option>
                <option value={900}>15m</option>
                <option value={1800}>30m</option>
                <option value={3600}>1h</option>
                <option value={0}>Off</option>
              </select>
            </label>
            <label className="setting-row">
              <span>Launch at login</span>
              <input
                type="checkbox"
                checked={settingsDocument.launchAtLogin}
                onChange={(event) => saveSettingsPatch({ launchAtLogin: event.target.checked })}
              />
            </label>
            <label className="setting-row">
              <span>Restore proxies on launch</span>
              <input
                type="checkbox"
                checked={settingsDocument.proxyAutoRestoreOnLaunch}
                onChange={(event) => saveSettingsPatch({ proxyAutoRestoreOnLaunch: event.target.checked })}
              />
            </label>
            <label className="setting-row">
              <span>Minimize to tray on close</span>
              <input
                type="checkbox"
                checked={settingsDocument.minimizeToTrayOnClose}
                onChange={(event) => saveSettingsPatch({ minimizeToTrayOnClose: event.target.checked })}
              />
            </label>
            <label className="setting-row">
              <span>Keep running in background</span>
              <input
                type="checkbox"
                checked={settingsDocument.keepRunningInBackground}
                onChange={(event) => saveSettingsPatch({ keepRunningInBackground: event.target.checked })}
              />
            </label>
            <div className="settings-path">{appSettings?.settingsPath || "%APPDATA%\\AIUsage\\settings.json"}</div>
            {appSettings?.autostartError ? <div className="settings-error">{appSettings.autostartError}</div> : null}
            <div className="diagnostics-row">
              <button
                className="action-button"
                type="button"
                onClick={exportDiagnostics}
                disabled={diagnosticsExporting}
              >
                <Download size={16} />
                <span>{diagnosticsExporting ? "Exporting" : "Export diagnostics"}</span>
              </button>
              {diagnosticsExport ? (
                <span>
                  {diagnosticsExport.recentFiles.length} files · {diagnosticsExport.exportPath}
                </span>
              ) : null}
            </div>
            {diagnosticsError ? <div className="settings-error">{diagnosticsError}</div> : null}
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
          <div className="updater-row">
            <div>
              <strong>{updaterStateLabel(updaterState)}</strong>
              <span>{updaterDetail}</span>
            </div>
            <span className={`status ${updaterStatusClass(updaterState)}`}>{updaterStateLabel(updaterState)}</span>
          </div>
          <div className="updater-actions">
            <button className="action-button" type="button" onClick={checkForUpdates} disabled={updaterBusy}>
              <RefreshCw size={16} />
              <span>{updaterState === "checking" ? "Checking" : "Check"}</span>
            </button>
            <button
              className="action-button primary-action"
              type="button"
              onClick={installPendingUpdate}
              disabled={!pendingUpdate || updaterBusy}
            >
              <Download size={16} />
              <span>{updaterState === "downloading" ? "Downloading" : "Install"}</span>
            </button>
          </div>
          {updaterProgress ? (
            <div className="updater-progress" aria-label="Update download progress">
              <div>
                <span style={{ width: `${updaterProgressPercent ?? 8}%` }} />
              </div>
              <strong>
                {formatBytes(updaterProgress.downloadedBytes)}
                {updaterProgress.contentLength ? ` / ${formatBytes(updaterProgress.contentLength)}` : ""}
              </strong>
            </div>
          ) : null}
          {updaterError ? <div className="settings-error">{updaterError}</div> : null}
        </section>

        <section className="panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Usage archive</p>
              <h3>Proxy token ledger</h3>
            </div>
            <BarChart3 size={18} />
          </div>
          <div className="usage-grid">
            <section className="usage-meter">
              <span>Requests</span>
              <strong>{proxyUsageStats?.totals.requests.toLocaleString() ?? "0"}</strong>
            </section>
            <section className="usage-meter">
              <span>Input</span>
              <strong>{proxyUsageStats?.totals.inputTokens.toLocaleString() ?? "0"}</strong>
            </section>
            <section className="usage-meter">
              <span>Output</span>
              <strong>{proxyUsageStats?.totals.outputTokens.toLocaleString() ?? "0"}</strong>
            </section>
            <section className="usage-meter">
              <span>Cache</span>
              <strong>
                {(
                  (proxyUsageStats?.totals.cacheReadTokens ?? 0) +
                  (proxyUsageStats?.totals.cacheWriteTokens ?? 0)
                ).toLocaleString()}
              </strong>
            </section>
          </div>
          <div className="surface-list usage-list">
            {(proxyUsageStats?.byModel.length ? proxyUsageStats.byModel.slice(0, 6) : [
              { track: "codex", model: "No usage recorded", totals: { requests: 0, inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0 } }
            ] satisfies ProxyUsageStats["byModel"]).map((row) => (
              <article key={`${row.track}-${row.model}`} className="surface-row">
                <div>
                  <strong>{row.model}</strong>
                  <span>{proxyTrackLabel(row.track)} · {row.totals.requests.toLocaleString()} requests</span>
                </div>
                <span className="usage-total">
                  {(
                    row.totals.inputTokens +
                    row.totals.outputTokens +
                    row.totals.cacheReadTokens +
                    row.totals.cacheWriteTokens
                  ).toLocaleString()}
                </span>
              </article>
            ))}
          </div>
        </section>

        <section className="panel compact-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Runtime health</p>
              <h3>Proxy supervisor</h3>
            </div>
            <ServerCog size={18} />
          </div>
          <div className="surface-list single-column">
            {(proxyHealth.length ? proxyHealth : [
              { track: "claudeCode", state: "stopped", listeningPort: null },
              { track: "codex", state: "stopped", listeningPort: null },
              { track: "openCode", state: "stopped", listeningPort: null },
              { track: "global", state: "stopped", listeningPort: null }
            ] satisfies ProxyHealth[]).map((health) => (
              <article key={health.track} className="surface-row">
                <div>
                  <strong>{proxyTrackLabel(health.track)}</strong>
                  <span>{health.listeningPort ? `127.0.0.1:${health.listeningPort}` : "Not listening"}</span>
                </div>
                <span className={`status runtime-${health.state}`}>{runtimeStateLabel(health.state)}</span>
              </article>
            ))}
          </div>
        </section>
      </section>
    </main>
  );
}
