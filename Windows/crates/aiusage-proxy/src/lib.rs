use aiusage_core::ProxyTrack;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyRuntimeState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyHealth {
    pub track: ProxyTrack,
    pub state: ProxyRuntimeState,
    pub listening_port: Option<u16>,
}

pub fn foundation_proxy_health() -> Vec<ProxyHealth> {
    [
        ProxyTrack::ClaudeCode,
        ProxyTrack::Codex,
        ProxyTrack::OpenCode,
        ProxyTrack::Global,
    ]
    .into_iter()
    .map(|track| ProxyHealth {
        track,
        state: ProxyRuntimeState::Stopped,
        listening_port: None,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_health_tracks_all_proxy_families() {
        assert_eq!(foundation_proxy_health().len(), 4);
    }
}
