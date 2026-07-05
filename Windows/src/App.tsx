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
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  Settings,
  ServerCog,
  ShieldCheck,
  Square,
  TerminalSquare,
  Trash2
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

type FeatureStatus = "planned" | "foundationReady" | "inProgress" | "complete" | "blocked";

type ProductSurface = {
  id: string;
  label: string;
  status: FeatureStatus;
};

type ProxyTrack = "claudeCode" | "codex" | "openCode" | "global";

type ProxyProtocol = "openAiResponses" | "openAiChatCompletions" | "anthropicMessages" | "passthrough";

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

type WslDistributionSummary = {
  name: string;
  homePath: string | null;
  claudeHome: string;
  codexHome: string;
  opencodeConfigDir: string;
  errorMessage: string | null;
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
  wslDistributions: WslDistributionSummary[];
  wslError: string | null;
  defaultProxyPorts: ProxyPortPreflight[];
};

type ManagedConfigKind = "claude" | "codex" | "openCode";

type ManagedConfigTargetKind = "nativeWindows" | "customPath" | "wslDistribution";

type ManagedConfigStatus = {
  kind: ManagedConfigKind;
  targetKind: ManagedConfigTargetKind;
  configPath: string;
  backupPath: string;
  configExists: boolean;
  backupExists: boolean;
  managed: boolean;
  usesJsonc: boolean;
  parseError: string | null;
};

type ProxyRuntimeState = "stopped" | "starting" | "running" | "stopping" | "failed";

type ProxyHealth = {
  track: ProxyTrack;
  state: ProxyRuntimeState;
  listeningPort: number | null;
};

type ProxyRuntimeDraft = {
  protocol: ProxyProtocol;
  bindHost: string;
  port: number;
  upstreamBaseUrl: string;
  upstreamApiKey: string;
  clientKey: string;
  defaultModel: string;
};

type ProxyRuntimeConfig = {
  track: ProxyTrack;
  nodeId: string;
  label: string;
  bindHost: string;
  port: number;
  upstreamBaseUrl: string;
  upstreamApiKey: string | null;
  clientKey: string | null;
  protocol: ProxyProtocol;
  defaultModel: string | null;
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

type LocalCertificateAuthoritySnapshot = {
  version: number;
  generatedAtEpochMs: number;
  certificateDir: string;
  certificateDerPath: string;
  certificatePemPath: string;
  privateKeyPath: string;
  certificateExists: boolean;
  privateKeyExists: boolean;
  sha256Thumbprint: string | null;
  trustedCurrentUserRoot: boolean;
  warningMessages: string[];
};

type CredentialSummary = {
  id: string;
  providerId: string;
  label: string;
  kind: CredentialKind;
  hasSecret: boolean;
  metadata: unknown;
  updatedAtEpochMs: number;
};

type CredentialKind = "apiKey" | "authFile" | "cookie" | "oAuth" | "token" | "webSession";

type CredentialProviderOption = {
  id: string;
  label: string;
  kinds: CredentialKind[];
};

type CredentialFormState = {
  providerId: string;
  label: string;
  kind: CredentialKind;
  secret: string;
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
    { id: "opencode", label: "OpenCode", status: "foundationReady" },
    { id: "antigravity", label: "Antigravity", status: "planned" },
    { id: "kiro", label: "Kiro", status: "planned" },
    { id: "warp", label: "Warp", status: "blocked" },
    { id: "droid", label: "Droid", status: "planned" },
    { id: "kimi", label: "Kimi", status: "planned" },
    { id: "minimax", label: "MiniMax", status: "planned" }
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

const credentialProviderOptions: CredentialProviderOption[] = [
  { id: "codex", label: "Codex", kinds: ["token", "authFile"] },
  { id: "copilot", label: "Copilot", kinds: ["token", "oAuth"] },
  { id: "cursor", label: "Cursor", kinds: ["cookie", "webSession"] },
  { id: "gemini", label: "Gemini CLI", kinds: ["authFile", "oAuth"] },
  { id: "opencode", label: "OpenCode", kinds: ["apiKey", "authFile"] },
  { id: "antigravity", label: "Antigravity", kinds: ["oAuth", "webSession"] },
  { id: "kiro", label: "Kiro", kinds: ["oAuth", "authFile"] },
  { id: "warp", label: "Warp", kinds: ["token", "authFile"] },
  { id: "droid", label: "Droid", kinds: ["authFile", "cookie"] },
  { id: "kimi", label: "Kimi", kinds: ["apiKey"] },
  { id: "minimax", label: "MiniMax", kinds: ["apiKey"] }
];

const allCredentialKinds: CredentialKind[] = ["apiKey", "authFile", "cookie", "oAuth", "token", "webSession"];

const defaultCredentialForm: CredentialFormState = {
  providerId: "opencode",
  label: "",
  kind: "apiKey",
  secret: ""
};

const defaultProxyDrafts: Record<ProxyTrack, ProxyRuntimeDraft> = {
  codex: {
    protocol: "openAiResponses",
    bindHost: "127.0.0.1",
    port: 14399,
    upstreamBaseUrl: "",
    upstreamApiKey: "",
    clientKey: "",
    defaultModel: "gpt-5"
  },
  claudeCode: {
    protocol: "anthropicMessages",
    bindHost: "127.0.0.1",
    port: 14400,
    upstreamBaseUrl: "",
    upstreamApiKey: "",
    clientKey: "",
    defaultModel: "claude-sonnet-4"
  },
  openCode: {
    protocol: "openAiChatCompletions",
    bindHost: "127.0.0.1",
    port: 14401,
    upstreamBaseUrl: "",
    upstreamApiKey: "",
    clientKey: "",
    defaultModel: ""
  },
  global: {
    protocol: "passthrough",
    bindHost: "127.0.0.1",
    port: 14402,
    upstreamBaseUrl: "",
    upstreamApiKey: "",
    clientKey: "",
    defaultModel: ""
  }
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

function proxyProtocolLabel(protocol: ProxyProtocol): string {
  switch (protocol) {
    case "anthropicMessages":
      return "Anthropic Messages";
    case "openAiChatCompletions":
      return "OpenAI Chat";
    case "passthrough":
      return "Passthrough";
    default:
      return "OpenAI Responses";
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

function managedConfigKindLabel(kind: ManagedConfigKind): string {
  switch (kind) {
    case "claude":
      return "Claude Code";
    case "openCode":
      return "OpenCode";
    default:
      return "Codex";
  }
}

function managedConfigTargetLabel(targetKind: ManagedConfigTargetKind): string {
  switch (targetKind) {
    case "customPath":
      return "Custom";
    case "wslDistribution":
      return "WSL";
    default:
      return "Native";
  }
}

function managedConfigStatusLabel(status: ManagedConfigStatus): string {
  if (status.parseError) {
    return "Warning";
  }
  if (status.managed) {
    return "Managed";
  }
  if (status.configExists) {
    return "Detected";
  }
  return "Missing";
}

function managedConfigStatusClass(status: ManagedConfigStatus): string {
  if (status.parseError) {
    return "blocked";
  }
  if (status.managed) {
    return "complete";
  }
  if (status.configExists) {
    return "inProgress";
  }
  return "planned";
}

function managedConfigRestoreCommand(kind: ManagedConfigKind): string {
  switch (kind) {
    case "claude":
      return "restore_claude_managed_config";
    case "openCode":
      return "restore_opencode_managed_config";
    default:
      return "restore_codex_managed_config";
  }
}

function canRestoreManagedConfig(status: ManagedConfigStatus): boolean {
  return status.managed || status.backupExists;
}

function proxyTrackForManagedConfig(kind: ManagedConfigKind): ProxyTrack {
  switch (kind) {
    case "claude":
      return "claudeCode";
    case "openCode":
      return "openCode";
    default:
      return "codex";
  }
}

function localProxyClientHost(bindHost: string): string {
  const host = bindHost.trim();
  if (!host || host === "0.0.0.0") {
    return "127.0.0.1";
  }
  if (host === "::" || host === "[::]") {
    return "[::1]";
  }
  return host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
}

function localProxyBaseUrl(draft: ProxyRuntimeDraft, suffix = ""): string {
  return `http://${localProxyClientHost(draft.bindHost)}:${draft.port}${suffix}`;
}

function proxyDraftHasValidPort(draft: ProxyRuntimeDraft): boolean {
  return Number.isInteger(draft.port) && draft.port > 0 && draft.port <= 65_535;
}

function credentialKindLabel(kind: CredentialKind): string {
  switch (kind) {
    case "apiKey":
      return "API key";
    case "authFile":
      return "Auth file";
    case "cookie":
      return "Cookie";
    case "oAuth":
      return "OAuth";
    case "webSession":
      return "Web session";
    default:
      return "Token";
  }
}

function credentialProviderLabel(providerId: string): string {
  return credentialProviderOptions.find((provider) => provider.id === providerId)?.label ?? providerId;
}

function formatEpochMs(epochMs: number): string {
  if (!epochMs) {
    return "Not synced";
  }
  return new Date(epochMs).toLocaleString();
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
  return formatTauriRuntimeError(error, "Updater");
}

function formatTauriRuntimeError(error: unknown, featureName: string): string {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("invoke") && message.includes("undefined")) {
    return `${featureName} is available in the packaged Windows app.`;
  }
  return message;
}

export function App() {
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>(fallbackSnapshot);
  const [platformEnvironment, setPlatformEnvironment] = useState<PlatformEnvironmentSnapshot | null>(null);
  const [proxyHealth, setProxyHealth] = useState<ProxyHealth[]>([]);
  const [activeProxyTrack, setActiveProxyTrack] = useState<ProxyTrack>("codex");
  const [proxyDrafts, setProxyDrafts] = useState<Record<ProxyTrack, ProxyRuntimeDraft>>(defaultProxyDrafts);
  const [proxyActionBusy, setProxyActionBusy] = useState<"preflight" | "start" | "stop" | null>(null);
  const [proxyActionError, setProxyActionError] = useState<string | null>(null);
  const [proxyPreflightResult, setProxyPreflightResult] = useState<ProxyPortPreflight | null>(null);
  const [proxyArchives, setProxyArchives] = useState<ProxyUsageArchiveSummary[]>([]);
  const [proxyUsageStats, setProxyUsageStats] = useState<ProxyUsageStats | null>(null);
  const [callInventory, setCallInventory] = useState<CallAnalyticsInventorySnapshot | null>(null);
  const [callSnapshot, setCallSnapshot] = useState<CallAnalyticsSnapshot | null>(null);
  const [managedConfigStatuses, setManagedConfigStatuses] = useState<ManagedConfigStatus[]>([]);
  const [managedConfigError, setManagedConfigError] = useState<string | null>(null);
  const [managedConfigBusy, setManagedConfigBusy] = useState<string | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettingsSnapshot | null>(null);
  const [diagnosticsExport, setDiagnosticsExport] = useState<DiagnosticsExportSnapshot | null>(null);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);
  const [diagnosticsExporting, setDiagnosticsExporting] = useState(false);
  const [localCertificateAuthority, setLocalCertificateAuthority] =
    useState<LocalCertificateAuthoritySnapshot | null>(null);
  const [localCertificateAuthorityError, setLocalCertificateAuthorityError] = useState<string | null>(null);
  const [localCertificateAuthorityBusy, setLocalCertificateAuthorityBusy] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<Update | null>(null);
  const [updaterState, setUpdaterState] = useState<UpdaterState>("idle");
  const [updaterError, setUpdaterError] = useState<string | null>(null);
  const [updaterProgress, setUpdaterProgress] = useState<UpdaterProgress | null>(null);
  const [credentials, setCredentials] = useState<CredentialSummary[]>([]);
  const [credentialForm, setCredentialForm] = useState<CredentialFormState>(defaultCredentialForm);
  const [credentialBusy, setCredentialBusy] = useState(false);
  const [credentialError, setCredentialError] = useState<string | null>(null);
  const [deletingCredentialId, setDeletingCredentialId] = useState<string | null>(null);
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
    invoke<ManagedConfigStatus[]>("config_statuses")
      .then(setManagedConfigStatuses)
      .catch((error) => setManagedConfigError(formatTauriRuntimeError(error, "Config takeover")));
    invoke<AppSettingsSnapshot>("app_settings")
      .then(setAppSettings)
      .catch(() => setAppSettings(null));
    invoke<LocalCertificateAuthoritySnapshot>("local_certificate_authority")
      .then(setLocalCertificateAuthority)
      .catch(() => setLocalCertificateAuthority(null));
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
  const activeProxyDraft = proxyDrafts[activeProxyTrack];
  const activeProxyHealth = proxyHealth.find((health) => health.track === activeProxyTrack);
  const activeProxyRunning = activeProxyHealth?.state === "running";
  const activeProxyPortValid = proxyDraftHasValidPort(activeProxyDraft);
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
    { label: "Native Claude", path: platformEnvironment?.paths.claudeHome ?? "%USERPROFILE%\\.claude" },
    { label: "Native Codex", path: platformEnvironment?.paths.codexHome ?? "%USERPROFILE%\\.codex" },
    {
      label: "Native OpenCode",
      path: platformEnvironment?.paths.opencodeConfigDir ?? "%USERPROFILE%\\.config\\opencode"
    }
  ];
  const systemProxy = platformEnvironment?.systemProxy;
  const browserProfiles = platformEnvironment?.browserProfiles ?? [];
  const wslDistributions = platformEnvironment?.wslDistributions ?? [];
  const defaultProxyPorts =
    platformEnvironment?.defaultProxyPorts ??
    ([
      { track: "codex", bindHost: "127.0.0.1", port: 14399, available: true, owner: null, errorMessage: null },
      { track: "claudeCode", bindHost: "127.0.0.1", port: 14400, available: true, owner: null, errorMessage: null },
      { track: "openCode", bindHost: "127.0.0.1", port: 14401, available: true, owner: null, errorMessage: null },
      { track: "global", bindHost: "127.0.0.1", port: 14402, available: true, owner: null, errorMessage: null }
    ] satisfies ProxyPortPreflight[]);
  const availablePlatformPaths = platformPathRows.filter((row) => !row.path.includes("%")).length;
  const localCaPrepared = Boolean(
    localCertificateAuthority?.certificateExists && localCertificateAuthority.privateKeyExists
  );
  const localCaTrusted = Boolean(localCertificateAuthority?.trustedCurrentUserRoot);
  const localCaStateLabel = localCaTrusted ? "Trusted" : localCaPrepared ? "Prepared" : "Not ready";
  const localCaStatusClass = localCaTrusted ? "complete" : localCaPrepared ? "inProgress" : "planned";
  const managedConfigManagedCount = managedConfigStatuses.filter((status) => status.managed).length;
  const managedConfigBackupCount = managedConfigStatuses.filter((status) => status.backupExists).length;
  const managedConfigWarningCount = managedConfigStatuses.filter((status) => status.parseError).length;
  const selectedCredentialProvider =
    credentialProviderOptions.find((provider) => provider.id === credentialForm.providerId) ??
    credentialProviderOptions[0];
  const credentialKindOptions = selectedCredentialProvider?.kinds ?? allCredentialKinds;
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

  function runLocalCertificateAuthorityAction(command: "ensure_local_ca" | "trust_local_ca") {
    setLocalCertificateAuthorityBusy(true);
    setLocalCertificateAuthorityError(null);
    invoke<LocalCertificateAuthoritySnapshot>(command)
      .then(setLocalCertificateAuthority)
      .catch((error) => setLocalCertificateAuthorityError(formatTauriRuntimeError(error, "Local CA")))
      .finally(() => setLocalCertificateAuthorityBusy(false));
  }

  function canActivateManagedConfig(status: ManagedConfigStatus): boolean {
    const draft = proxyDrafts[proxyTrackForManagedConfig(status.kind)];
    if (!draft.bindHost.trim() || !proxyDraftHasValidPort(draft)) {
      return false;
    }
    return status.kind !== "openCode" || Boolean(draft.defaultModel.trim());
  }

  function upsertManagedConfigStatus(updated: ManagedConfigStatus) {
    setManagedConfigStatuses((previous) =>
      previous.map((row) =>
        row.kind === updated.kind && row.configPath === updated.configPath
          ? { ...updated, targetKind: row.targetKind }
          : row
      )
    );
  }

  function applyManagedConfig(status: ManagedConfigStatus) {
    const track = proxyTrackForManagedConfig(status.kind);
    const draft = proxyDrafts[track];
    const configPath = status.targetKind === "nativeWindows" ? null : status.configPath;
    let command = "apply_codex_config";
    let request: unknown;

    if (status.kind === "claude") {
      command = "apply_claude_config";
      request = {
        settings: {
          baseUrl: localProxyBaseUrl(draft),
          authToken: draft.clientKey.trim() || null,
          defaultModel: draft.defaultModel.trim() || null,
          opusModel: null,
          sonnetModel: null,
          haikuModel: null,
          nodeExtraCaCerts: null
        },
        configPath
      };
    } else if (status.kind === "openCode") {
      command = "apply_opencode_config";
      request = {
        node: {
          managedProviderId: "aiusage-main",
          displayName: "AIUsage Main",
          npmPackage: "@ai-sdk/openai-compatible",
          baseUrl: localProxyBaseUrl(draft, "/v1"),
          apiKey: draft.clientKey.trim() || null,
          defaultModel: draft.defaultModel.trim(),
          models: [{ id: draft.defaultModel.trim(), displayName: draft.defaultModel.trim() }]
        },
        commonSettings: null,
        configPath
      };
    } else {
      request = {
        baseUrl: localProxyBaseUrl(draft, "/v1"),
        bearerToken: draft.clientKey.trim(),
        model: draft.defaultModel.trim() || "gpt-5",
        globalToml: "",
        nodeToml: "",
        configPath
      };
    }

    setManagedConfigBusy(status.kind);
    setManagedConfigError(null);
    invoke<ManagedConfigStatus>(command, { request })
      .then(upsertManagedConfigStatus)
      .catch((error) => setManagedConfigError(formatTauriRuntimeError(error, "Config takeover")))
      .finally(() => setManagedConfigBusy(null));
  }

  function runManagedConfigRestore(status: ManagedConfigStatus) {
    const command = managedConfigRestoreCommand(status.kind);
    setManagedConfigBusy(status.kind);
    setManagedConfigError(null);
    invoke<ManagedConfigStatus>(command, {
      configPath: status.targetKind === "nativeWindows" ? null : status.configPath
    })
      .then(upsertManagedConfigStatus)
      .catch((error) => setManagedConfigError(formatTauriRuntimeError(error, "Config takeover")))
      .finally(() => setManagedConfigBusy(null));
  }

  function updateCredentialProvider(providerId: string) {
    const provider = credentialProviderOptions.find((option) => option.id === providerId);
    setCredentialForm((previous) => ({
      ...previous,
      providerId,
      kind: provider?.kinds[0] ?? previous.kind
    }));
  }

  function saveCredential() {
    setCredentialBusy(true);
    setCredentialError(null);
    invoke<CredentialSummary>("save_provider_credential", {
      request: {
        providerId: credentialForm.providerId,
        label: credentialForm.label,
        kind: credentialForm.kind,
        secret: credentialForm.secret,
        metadata: { source: "windows-ui" }
      }
    })
      .then((summary) => {
        setCredentials((previous) => [summary, ...previous.filter((credential) => credential.id !== summary.id)]);
        setCredentialForm(defaultCredentialForm);
      })
      .catch((error) => setCredentialError(formatTauriRuntimeError(error, "Credential vault")))
      .finally(() => setCredentialBusy(false));
  }

  function deleteCredentialSummary(summary: CredentialSummary) {
    setDeletingCredentialId(summary.id);
    setCredentialError(null);
    invoke<boolean>("delete_provider_credential", { id: summary.id })
      .then((removed) => {
        if (removed) {
          setCredentials((previous) => previous.filter((credential) => credential.id !== summary.id));
        }
      })
      .catch((error) => setCredentialError(formatTauriRuntimeError(error, "Credential vault")))
      .finally(() => setDeletingCredentialId(null));
  }

  function updateProxyDraft(track: ProxyTrack, patch: Partial<ProxyRuntimeDraft>) {
    setProxyDrafts((previous) => ({
      ...previous,
      [track]: {
        ...previous[track],
        ...patch
      }
    }));
    setProxyPreflightResult(null);
  }

  function upsertProxyHealth(updated: ProxyHealth) {
    setProxyHealth((previous) => {
      const next = previous.filter((health) => health.track !== updated.track);
      next.push(updated);
      return next;
    });
  }

  function buildProxyRuntimeConfig(track: ProxyTrack, draft: ProxyRuntimeDraft): ProxyRuntimeConfig {
    return {
      track,
      nodeId: `${track}-local`,
      label: `${proxyTrackLabel(track)} local proxy`,
      bindHost: draft.bindHost.trim(),
      port: draft.port,
      upstreamBaseUrl: draft.upstreamBaseUrl.trim(),
      upstreamApiKey: draft.upstreamApiKey.trim() || null,
      clientKey: draft.clientKey.trim() || null,
      protocol: draft.protocol,
      defaultModel: draft.defaultModel.trim() || null
    };
  }

  function runProxyPortPreflight() {
    setProxyActionBusy("preflight");
    setProxyActionError(null);
    invoke<ProxyPortPreflight>("proxy_port_preflight", {
      track: activeProxyTrack,
      bindHost: activeProxyDraft.bindHost,
      port: activeProxyDraft.port
    })
      .then(setProxyPreflightResult)
      .catch((error) => setProxyActionError(formatTauriRuntimeError(error, "Proxy runtime")))
      .finally(() => setProxyActionBusy(null));
  }

  async function startSelectedProxy() {
    setProxyActionBusy("start");
    setProxyActionError(null);
    try {
      const preflight = await invoke<ProxyPortPreflight>("proxy_port_preflight", {
        track: activeProxyTrack,
        bindHost: activeProxyDraft.bindHost,
        port: activeProxyDraft.port
      });
      setProxyPreflightResult(preflight);
      if (!preflight.available) {
        setProxyActionError(
          preflight.owner
            ? `Port ${preflight.port} is owned by PID ${preflight.owner.processId}`
            : preflight.errorMessage ?? `Port ${preflight.port} is busy`
        );
        return;
      }

      const health = await invoke<ProxyHealth>("start_proxy", {
        config: buildProxyRuntimeConfig(activeProxyTrack, activeProxyDraft)
      });
      upsertProxyHealth(health);
    } catch (error) {
      setProxyActionError(formatTauriRuntimeError(error, "Proxy runtime"));
    } finally {
      setProxyActionBusy(null);
    }
  }

  function stopSelectedProxy() {
    setProxyActionBusy("stop");
    setProxyActionError(null);
    invoke<ProxyHealth>("stop_proxy", { track: activeProxyTrack })
      .then(upsertProxyHealth)
      .catch((error) => setProxyActionError(formatTauriRuntimeError(error, "Proxy runtime")))
      .finally(() => setProxyActionBusy(null));
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
              <span>WSL distros</span>
              <strong>{wslDistributions.length}</strong>
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
            <section className="usage-meter">
              <span>Local CA</span>
              <strong>{localCaStateLabel}</strong>
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
            {(wslDistributions.length
              ? wslDistributions.slice(0, 4)
              : [
                  {
                    name: "Not detected",
                    homePath: null,
                    claudeHome: "~/.claude",
                    codexHome: "~/.codex",
                    opencodeConfigDir: "~/.config/opencode",
                    errorMessage: null
                  }
                ]
            ).map((distribution) => {
              const detected = distribution.name !== "Not detected";
              return (
                <article key={`wsl-${distribution.name}`} className="surface-row">
                  <div>
                    <strong>WSL · {distribution.name}</strong>
                    <span>
                      Claude {distribution.claudeHome} · Codex {distribution.codexHome}
                    </span>
                    <span>OpenCode {distribution.opencodeConfigDir}</span>
                    {distribution.errorMessage ? <span>{distribution.errorMessage}</span> : null}
                  </div>
                  <span
                    className={`status ${
                      distribution.errorMessage ? "blocked" : detected ? "complete" : "runtime-stopped"
                    }`}
                  >
                    {distribution.errorMessage ? "Warning" : detected ? "Detected" : "None"}
                  </span>
                </article>
              );
            })}
            <article className="surface-row">
              <div>
                <strong>System proxy</strong>
                <span>{systemProxy?.http ?? systemProxy?.https ?? systemProxy?.socks ?? "Not configured"}</span>
              </div>
              <span className={`status ${systemProxy?.anyEnabled ? "runtime-running" : "runtime-stopped"}`}>
                {systemProxy?.anyEnabled ? "Enabled" : "Disabled"}
              </span>
            </article>
            <article className="surface-row certificate-row">
              <div>
                <strong>Local HTTPS CA</strong>
                <span>
                  {localCertificateAuthority?.sha256Thumbprint ??
                    localCertificateAuthority?.certificateDerPath ??
                    "%APPDATA%\\AIUsage\\certificates"}
                </span>
                {localCertificateAuthority?.certificateDerPath ? (
                  <span>{localCertificateAuthority.certificateDerPath}</span>
                ) : null}
              </div>
              <span className={`status ${localCaStatusClass}`}>{localCaStateLabel}</span>
            </article>
            <div className="certificate-actions">
              <button
                className="action-button"
                type="button"
                onClick={() => runLocalCertificateAuthorityAction("ensure_local_ca")}
                disabled={localCertificateAuthorityBusy}
              >
                <ShieldCheck size={16} />
                <span>{localCertificateAuthorityBusy ? "Working" : "Prepare CA"}</span>
              </button>
              <button
                className="action-button primary-action"
                type="button"
                onClick={() => runLocalCertificateAuthorityAction("trust_local_ca")}
                disabled={localCertificateAuthorityBusy || !localCaPrepared || localCaTrusted}
              >
                <ShieldCheck size={16} />
                <span>{localCaTrusted ? "Trusted" : "Trust CA"}</span>
              </button>
            </div>
            {localCertificateAuthority?.warningMessages.length ? (
              <div className="settings-error">{localCertificateAuthority.warningMessages.join(" · ")}</div>
            ) : null}
            {localCertificateAuthorityError ? (
              <div className="settings-error">{localCertificateAuthorityError}</div>
            ) : null}
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
          {platformEnvironment?.wslError ? <div className="settings-error">{platformEnvironment.wslError}</div> : null}
        </section>

        <section className="panel compact-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Target safety</p>
              <h3>Config takeover</h3>
            </div>
            <Braces size={18} />
          </div>
          <div className="usage-grid">
            <section className="usage-meter">
              <span>Managed</span>
              <strong>
                {managedConfigManagedCount}/{managedConfigStatuses.length || 3}
              </strong>
            </section>
            <section className="usage-meter">
              <span>Backups</span>
              <strong>{managedConfigBackupCount}</strong>
            </section>
            <section className="usage-meter">
              <span>Warnings</span>
              <strong>{managedConfigWarningCount}</strong>
            </section>
          </div>
          <div className="surface-list single-column">
            {(managedConfigStatuses.length
              ? managedConfigStatuses
              : [
                  {
                    kind: "codex",
                    targetKind: "nativeWindows",
                    configPath: "%USERPROFILE%\\.codex\\config.toml",
                    backupPath: "%USERPROFILE%\\.codex\\config.toml.aiusage.bak",
                    configExists: false,
                    backupExists: false,
                    managed: false,
                    usesJsonc: false,
                    parseError: null
                  },
                  {
                    kind: "claude",
                    targetKind: "nativeWindows",
                    configPath: "%USERPROFILE%\\.claude\\settings.json",
                    backupPath: "%USERPROFILE%\\.claude\\settings.json.aiusage.bak",
                    configExists: false,
                    backupExists: false,
                    managed: false,
                    usesJsonc: false,
                    parseError: null
                  },
                  {
                    kind: "openCode",
                    targetKind: "nativeWindows",
                    configPath: "%USERPROFILE%\\.config\\opencode\\opencode.json",
                    backupPath: "%USERPROFILE%\\.config\\opencode\\opencode.json.aiusage.bak",
                    configExists: false,
                    backupExists: false,
                    managed: false,
                    usesJsonc: false,
                    parseError: null
                  }
                ] satisfies ManagedConfigStatus[]
            ).map((status) => (
              <article key={`${status.kind}-${status.targetKind}-${status.configPath}`} className="surface-row config-row">
                <div>
                  <strong>
                    {managedConfigKindLabel(status.kind)} · {managedConfigTargetLabel(status.targetKind)}
                  </strong>
                  <span>{status.configPath}</span>
                  <span>
                    {status.backupExists ? `Backup ${status.backupPath}` : "No backup"}
                    {status.usesJsonc ? " · JSONC" : ""}
                  </span>
                  {status.parseError ? <span>{status.parseError}</span> : null}
                </div>
                <div className="row-actions">
                  <span className={`status ${managedConfigStatusClass(status)}`}>{managedConfigStatusLabel(status)}</span>
                  <button
                    className="icon-action"
                    type="button"
                    title={`Apply ${managedConfigKindLabel(status.kind)}`}
                    aria-label={`Apply ${managedConfigKindLabel(status.kind)}`}
                    disabled={managedConfigBusy === status.kind || !canActivateManagedConfig(status)}
                    onClick={() => applyManagedConfig(status)}
                  >
                    <Save size={15} />
                  </button>
                  <button
                    className="icon-action"
                    type="button"
                    title={`Restore ${managedConfigKindLabel(status.kind)}`}
                    aria-label={`Restore ${managedConfigKindLabel(status.kind)}`}
                    disabled={managedConfigBusy === status.kind || !canRestoreManagedConfig(status)}
                    onClick={() => runManagedConfigRestore(status)}
                  >
                    <RotateCcw size={15} />
                  </button>
                </div>
              </article>
            ))}
          </div>
          {managedConfigError ? <div className="settings-error">{managedConfigError}</div> : null}
        </section>

        <section className="panel compact-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Credential vault</p>
              <h3>Provider credentials</h3>
            </div>
            <KeyRound size={18} />
          </div>
          <div className="settings-list credential-form">
            <label className="setting-row">
              <span>Provider</span>
              <select value={credentialForm.providerId} onChange={(event) => updateCredentialProvider(event.target.value)}>
                {credentialProviderOptions.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="setting-row">
              <span>Kind</span>
              <select
                value={credentialForm.kind}
                onChange={(event) =>
                  setCredentialForm((previous) => ({
                    ...previous,
                    kind: event.target.value as CredentialKind
                  }))
                }
              >
                {credentialKindOptions.map((kind) => (
                  <option key={kind} value={kind}>
                    {credentialKindLabel(kind)}
                  </option>
                ))}
              </select>
            </label>
            <label className="setting-row">
              <span>Label</span>
              <input
                type="text"
                value={credentialForm.label}
                onChange={(event) =>
                  setCredentialForm((previous) => ({
                    ...previous,
                    label: event.target.value
                  }))
                }
              />
            </label>
            <label className="setting-row">
              <span>Secret</span>
              <input
                type="password"
                value={credentialForm.secret}
                autoComplete="new-password"
                onChange={(event) =>
                  setCredentialForm((previous) => ({
                    ...previous,
                    secret: event.target.value
                  }))
                }
              />
            </label>
            <button
              className="action-button primary-action"
              type="button"
              onClick={saveCredential}
              disabled={credentialBusy || !credentialForm.label.trim() || !credentialForm.secret.trim()}
            >
              <Save size={16} />
              <span>{credentialBusy ? "Saving" : "Save credential"}</span>
            </button>
          </div>
          <div className="surface-list single-column credential-list">
            {credentials.length ? (
              credentials.map((credential) => (
                <article key={credential.id} className="surface-row">
                  <div>
                    <strong>
                      {credentialProviderLabel(credential.providerId)} · {credential.label}
                    </strong>
                    <span>
                      {credentialKindLabel(credential.kind)} · {formatEpochMs(credential.updatedAtEpochMs)}
                    </span>
                    <span>{credential.hasSecret ? "Secret stored" : "No secret stored"}</span>
                  </div>
                  <div className="row-actions">
                    <span className={`status ${credential.hasSecret ? "complete" : "planned"}`}>
                      {credential.hasSecret ? "Stored" : "Empty"}
                    </span>
                    <button
                      className="icon-action"
                      type="button"
                      title={`Delete ${credential.label}`}
                      aria-label={`Delete ${credential.label}`}
                      disabled={deletingCredentialId === credential.id}
                      onClick={() => deleteCredentialSummary(credential)}
                    >
                      <Trash2 size={15} />
                    </button>
                  </div>
                </article>
              ))
            ) : (
              <article className="surface-row">
                <div>
                  <strong>No credentials</strong>
                  <span>Windows Credential Manager / DPAPI vault</span>
                </div>
                <span className="status planned">Empty</span>
              </article>
            )}
          </div>
          {credentialError ? <div className="settings-error">{credentialError}</div> : null}
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
          <div className="settings-list proxy-control-form">
            <label className="setting-row">
              <span>Track</span>
              <select
                value={activeProxyTrack}
                onChange={(event) => {
                  setActiveProxyTrack(event.target.value as ProxyTrack);
                  setProxyPreflightResult(null);
                }}
              >
                {(["codex", "claudeCode", "openCode", "global"] satisfies ProxyTrack[]).map((track) => (
                  <option key={track} value={track}>
                    {proxyTrackLabel(track)}
                  </option>
                ))}
              </select>
            </label>
            <label className="setting-row">
              <span>Protocol</span>
              <select
                value={activeProxyDraft.protocol}
                onChange={(event) =>
                  updateProxyDraft(activeProxyTrack, { protocol: event.target.value as ProxyProtocol })
                }
              >
                {(["openAiResponses", "openAiChatCompletions", "anthropicMessages", "passthrough"] satisfies ProxyProtocol[]).map(
                  (protocol) => (
                    <option key={protocol} value={protocol}>
                      {proxyProtocolLabel(protocol)}
                    </option>
                  )
                )}
              </select>
            </label>
            <label className="setting-row">
              <span>Bind host</span>
              <input
                type="text"
                value={activeProxyDraft.bindHost}
                onChange={(event) => updateProxyDraft(activeProxyTrack, { bindHost: event.target.value })}
              />
            </label>
            <label className="setting-row">
              <span>Port</span>
              <input
                type="number"
                min={1}
                max={65535}
                value={activeProxyDraft.port}
                onChange={(event) => updateProxyDraft(activeProxyTrack, { port: Number(event.target.value) })}
              />
            </label>
            <label className="setting-row">
              <span>Upstream URL</span>
              <input
                type="text"
                value={activeProxyDraft.upstreamBaseUrl}
                onChange={(event) => updateProxyDraft(activeProxyTrack, { upstreamBaseUrl: event.target.value })}
              />
            </label>
            <label className="setting-row">
              <span>Upstream key</span>
              <input
                type="password"
                value={activeProxyDraft.upstreamApiKey}
                autoComplete="new-password"
                onChange={(event) => updateProxyDraft(activeProxyTrack, { upstreamApiKey: event.target.value })}
              />
            </label>
            <label className="setting-row">
              <span>Client key</span>
              <input
                type="password"
                value={activeProxyDraft.clientKey}
                autoComplete="new-password"
                onChange={(event) => updateProxyDraft(activeProxyTrack, { clientKey: event.target.value })}
              />
            </label>
            <label className="setting-row">
              <span>Model</span>
              <input
                type="text"
                value={activeProxyDraft.defaultModel}
                onChange={(event) => updateProxyDraft(activeProxyTrack, { defaultModel: event.target.value })}
              />
            </label>
            <div className="proxy-actions">
              <button
                className="action-button"
                type="button"
                onClick={runProxyPortPreflight}
                disabled={proxyActionBusy !== null || !activeProxyDraft.bindHost.trim() || !activeProxyPortValid}
              >
                <Gauge size={16} />
                <span>{proxyActionBusy === "preflight" ? "Checking" : "Preflight"}</span>
              </button>
              <button
                className="action-button primary-action"
                type="button"
                onClick={startSelectedProxy}
                disabled={
                  proxyActionBusy !== null ||
                  activeProxyRunning ||
                  !activeProxyDraft.bindHost.trim() ||
                  !activeProxyPortValid ||
                  !activeProxyDraft.upstreamBaseUrl.trim()
                }
              >
                <Play size={16} />
                <span>{proxyActionBusy === "start" ? "Starting" : "Start"}</span>
              </button>
              <button
                className="action-button"
                type="button"
                onClick={stopSelectedProxy}
                disabled={proxyActionBusy !== null || !activeProxyRunning}
              >
                <Square size={16} />
                <span>{proxyActionBusy === "stop" ? "Stopping" : "Stop"}</span>
              </button>
            </div>
            {proxyPreflightResult ? (
              <article className="surface-row">
                <div>
                  <strong>
                    {proxyTrackLabel(proxyPreflightResult.track)} · {proxyPreflightResult.bindHost}:{proxyPreflightResult.port}
                  </strong>
                  <span>
                    {proxyPreflightResult.owner
                      ? `PID ${proxyPreflightResult.owner.processId}${
                          proxyPreflightResult.owner.imagePath ? ` · ${proxyPreflightResult.owner.imagePath}` : ""
                        }`
                      : proxyPreflightResult.errorMessage ?? "Available"}
                  </span>
                </div>
                <span className={`status ${proxyPreflightResult.available ? "runtime-running" : "runtime-failed"}`}>
                  {proxyPreflightResult.available ? "Free" : "Busy"}
                </span>
              </article>
            ) : null}
            {proxyActionError ? <div className="settings-error">{proxyActionError}</div> : null}
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
