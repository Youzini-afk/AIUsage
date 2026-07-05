use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatureStatus {
    Planned,
    FoundationReady,
    InProgress,
    Complete,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSurface {
    pub id: String,
    pub label: String,
    pub status: FeatureStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderContract {
    pub id: String,
    pub label: String,
    pub status: FeatureStatus,
    pub credential_kinds: Vec<CredentialKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialKind {
    ApiKey,
    AuthFile,
    Cookie,
    OAuth,
    Token,
    WebSession,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyTrack {
    ClaudeCode,
    Codex,
    OpenCode,
    Global,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetric {
    pub label: String,
    pub remaining_percent: Option<f64>,
    pub used_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub provider_id: String,
    pub account_label: String,
    pub metrics: Vec<UsageMetric>,
    pub refreshed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyNode {
    pub id: String,
    pub label: String,
    pub track: ProxyTrack,
    pub protocol: String,
    pub base_url: String,
    pub default_model: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageArchiveRecord {
    pub day: String,
    pub track: ProxyTrack,
    pub node_id: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsSummary {
    pub source: String,
    pub tool_calls: u64,
    pub mcp_calls: u64,
    pub skill_calls: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSnapshot {
    pub app_name: String,
    pub phase: String,
    pub surfaces: Vec<ProductSurface>,
    pub providers: Vec<ProductSurface>,
    pub release_targets: Vec<String>,
}
