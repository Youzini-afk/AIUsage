use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeManagedNode {
    pub managed_provider_id: String,
    pub display_name: String,
    pub npm_package: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub default_model: String,
    pub models: Vec<OpenCodeManagedModel>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeManagedModel {
    pub id: String,
    pub display_name: Option<String>,
}

pub const OPENCODE_PROVIDER_PREFIX: &str = "aiusage";

pub fn is_managed_opencode_provider_key(key: &str) -> bool {
    key == OPENCODE_PROVIDER_PREFIX || key.starts_with(&format!("{OPENCODE_PROVIDER_PREFIX}-"))
}

pub fn strip_opencode_managed_entries(root: &Value) -> Value {
    let mut result = root.as_object().cloned().unwrap_or_default();

    if let Some(provider) = result.get_mut("provider").and_then(Value::as_object_mut) {
        provider.retain(|key, _| !is_managed_opencode_provider_key(key));
        if provider.is_empty() {
            result.remove("provider");
        }
    }

    if let Some(model) = result.get("model").and_then(Value::as_str) {
        if model
            .split_once('/')
            .map(|(provider, _)| is_managed_opencode_provider_key(provider))
            .unwrap_or(false)
        {
            result.remove("model");
        }
    }

    Value::Object(result)
}

pub fn opencode_has_managed_entries(root: &Value) -> bool {
    if let Some(provider) = root.get("provider").and_then(Value::as_object) {
        if provider
            .keys()
            .any(|key| is_managed_opencode_provider_key(key))
        {
            return true;
        }
    }

    root.get("model")
        .and_then(Value::as_str)
        .and_then(|model| model.split_once('/').map(|(provider, _)| provider))
        .map(is_managed_opencode_provider_key)
        .unwrap_or(false)
}

pub fn deep_merge_json(base: &Value, overlay: &Value) -> Value {
    match (base, overlay) {
        (Value::Object(base_object), Value::Object(overlay_object)) => {
            let mut merged = base_object.clone();
            for (key, value) in overlay_object {
                let next = merged
                    .get(key)
                    .map(|existing| deep_merge_json(existing, value))
                    .unwrap_or_else(|| value.clone());
                merged.insert(key.clone(), next);
            }
            Value::Object(merged)
        }
        (_, overlay) => overlay.clone(),
    }
}

pub fn inject_opencode_managed_config_with_base(
    root: &Value,
    base_settings: Option<&Value>,
    node: &OpenCodeManagedNode,
) -> Value {
    let clean = strip_opencode_managed_entries(root);
    let merged = base_settings
        .map(strip_opencode_managed_entries)
        .map(|base| deep_merge_json(&clean, &base))
        .unwrap_or(clean);
    inject_opencode_managed_config(&merged, node)
}

pub fn inject_opencode_managed_config(root: &Value, node: &OpenCodeManagedNode) -> Value {
    let mut result = strip_opencode_managed_entries(root)
        .as_object()
        .cloned()
        .unwrap_or_default();
    result
        .entry("$schema")
        .or_insert_with(|| Value::String("https://opencode.ai/config.json".into()));

    let mut provider = result
        .remove("provider")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    provider.insert(
        node.managed_provider_id.clone(),
        Value::Object(managed_provider_entry(node)),
    );

    result.insert("provider".into(), Value::Object(provider));
    result.insert(
        "model".into(),
        Value::String(format!(
            "{}/{}",
            node.managed_provider_id, node.default_model
        )),
    );

    Value::Object(result)
}

fn managed_provider_entry(node: &OpenCodeManagedNode) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("npm".into(), Value::String(node.npm_package.clone()));
    entry.insert("name".into(), Value::String(node.display_name.clone()));

    let mut options = Map::new();
    options.insert("baseURL".into(), Value::String(node.base_url.clone()));
    if let Some(api_key) = &node.api_key {
        options.insert("apiKey".into(), Value::String(api_key.clone()));
    }
    entry.insert("options".into(), Value::Object(options));

    let mut models = Map::new();
    for model in &node.models {
        let mut model_entry = Map::new();
        model_entry.insert(
            "name".into(),
            Value::String(
                model
                    .display_name
                    .clone()
                    .unwrap_or_else(|| model.id.clone()),
            ),
        );
        models.insert(model.id.clone(), Value::Object(model_entry));
    }
    entry.insert("models".into(), Value::Object(models));

    entry
}
