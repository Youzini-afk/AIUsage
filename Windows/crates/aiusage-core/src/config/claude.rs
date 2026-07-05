use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const CLAUDE_MANAGED_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "NODE_EXTRA_CA_CERTS",
];

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeManagedSettings {
    pub base_url: Option<String>,
    pub auth_token: Option<String>,
    pub default_model: Option<String>,
    pub opus_model: Option<String>,
    pub sonnet_model: Option<String>,
    pub haiku_model: Option<String>,
    pub node_extra_ca_certs: Option<String>,
}

pub fn claude_has_managed_entries(root: &Value) -> bool {
    root.get("env")
        .and_then(Value::as_object)
        .map(|env| {
            env.keys()
                .any(|key| CLAUDE_MANAGED_ENV_KEYS.contains(&key.as_str()))
        })
        .unwrap_or(false)
}

pub fn strip_claude_managed_settings(root: &Value) -> Value {
    let mut result = root.as_object().cloned().unwrap_or_default();
    if let Some(env) = result.get_mut("env").and_then(Value::as_object_mut) {
        env.retain(|key, _| !CLAUDE_MANAGED_ENV_KEYS.contains(&key.as_str()));
        if env.is_empty() {
            result.remove("env");
        }
    }
    result.remove("model");
    Value::Object(result)
}

pub fn inject_claude_managed_settings(root: &Value, settings: &ClaudeManagedSettings) -> Value {
    let mut result = strip_claude_managed_settings(root)
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut env = result
        .remove("env")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    upsert_env(&mut env, "ANTHROPIC_BASE_URL", &settings.base_url);
    upsert_env(&mut env, "ANTHROPIC_AUTH_TOKEN", &settings.auth_token);
    upsert_env(
        &mut env,
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        &settings.opus_model,
    );
    upsert_env(
        &mut env,
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        &settings.sonnet_model,
    );
    upsert_env(
        &mut env,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        &settings.haiku_model,
    );
    upsert_env(
        &mut env,
        "NODE_EXTRA_CA_CERTS",
        &settings.node_extra_ca_certs,
    );

    if env.is_empty() {
        result.remove("env");
    } else {
        result.insert("env".into(), Value::Object(env));
    }

    match settings.default_model.as_deref().map(str::trim) {
        Some(model) if !model.is_empty() => {
            result.insert("model".into(), Value::String(model.to_string()));
        }
        _ => {
            result.remove("model");
        }
    }

    Value::Object(result)
}

fn upsert_env(env: &mut Map<String, Value>, key: &str, value: &Option<String>) {
    match value.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => {
            env.insert(key.into(), Value::String(value.to_string()));
        }
        _ => {
            env.remove(key);
        }
    }
}
