use aiusage_core::{
    inject_claude_managed_settings, inject_codex_managed_config, inject_opencode_managed_config,
    parse_json_or_jsonc, strip_claude_managed_settings, strip_codex_managed_blocks,
    strip_opencode_managed_entries, ClaudeManagedSettings, CodexManagedConfig,
    OpenCodeManagedModel, OpenCodeManagedNode,
};
use serde_json::json;

#[test]
fn codex_transform_injects_managed_blocks_and_restores_clean_body() {
    let original = r#"
model = "old-model"
model_provider = "openai"
model_reasoning_effort = "medium"

[mcp_servers.demo]
command = "node"

[model_providers.openai]
base_url = "https://api.openai.com/v1"
"#;

    let injected = inject_codex_managed_config(
        original,
        CodexManagedConfig {
            base_url: "http://127.0.0.1:4317/v1",
            bearer_token: "client-key",
            model: "gpt-5",
            global_toml: "model_reasoning_effort = \"high\"",
            node_toml: "[mcp_servers.demo]\ncommand = \"pnpm\"",
        },
    );

    assert!(injected.contains("model = \"gpt-5\""));
    assert!(injected.contains("model_provider = \"aiusage-proxy\""));
    assert!(injected.contains("experimental_bearer_token = \"client-key\""));
    assert!(injected.contains("model_reasoning_effort = \"high\""));
    assert!(injected.contains("command = \"pnpm\""));
    assert!(!injected.contains("model = \"old-model\""));

    let restored = strip_codex_managed_blocks(&injected);
    assert!(restored.contains("[model_providers.openai]"));
    assert!(!restored.contains("AIUSAGE-CODEX"));
}

#[test]
fn opencode_transform_injects_provider_and_strips_managed_entries() {
    let original = json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {
            "anthropic": {
                "npm": "@ai-sdk/anthropic",
                "models": {}
            },
            "aiusage-old": {
                "npm": "@ai-sdk/openai",
                "models": {}
            }
        },
        "model": "aiusage-old/old-model",
        "theme": "system"
    });

    let node = OpenCodeManagedNode {
        managed_provider_id: "aiusage-main".into(),
        display_name: "AIUsage Main".into(),
        npm_package: "@ai-sdk/openai-compatible".into(),
        base_url: "http://127.0.0.1:4321/v1".into(),
        api_key: Some("client-key".into()),
        default_model: "gpt-5".into(),
        models: vec![OpenCodeManagedModel {
            id: "gpt-5".into(),
            display_name: None,
        }],
    };

    let injected = inject_opencode_managed_config(&original, &node);
    assert_eq!(injected["model"], "aiusage-main/gpt-5");
    assert_eq!(
        injected["provider"]["aiusage-main"]["options"]["baseURL"],
        "http://127.0.0.1:4321/v1"
    );
    assert!(injected["provider"].get("anthropic").is_some());
    assert!(injected["provider"].get("aiusage-old").is_none());

    let stripped = strip_opencode_managed_entries(&injected);
    assert!(stripped["provider"].get("anthropic").is_some());
    assert!(stripped["provider"].get("aiusage-main").is_none());
    assert!(stripped.get("model").is_none());
}

#[test]
fn claude_transform_manages_env_keys_and_restores_user_settings() {
    let original = json!({
        "env": {
            "PATH": "keep",
            "ANTHROPIC_BASE_URL": "https://old.example"
        },
        "model": "old-model",
        "permissions": {
            "allow": ["Bash(ls)"]
        }
    });

    let injected = inject_claude_managed_settings(
        &original,
        &ClaudeManagedSettings {
            base_url: Some("http://127.0.0.1:4315".into()),
            auth_token: Some("client-key".into()),
            default_model: Some("claude-sonnet-4".into()),
            sonnet_model: Some("claude-sonnet-4".into()),
            ..Default::default()
        },
    );

    assert_eq!(injected["env"]["PATH"], "keep");
    assert_eq!(
        injected["env"]["ANTHROPIC_BASE_URL"],
        "http://127.0.0.1:4315"
    );
    assert_eq!(injected["env"]["ANTHROPIC_AUTH_TOKEN"], "client-key");
    assert_eq!(injected["model"], "claude-sonnet-4");

    let stripped = strip_claude_managed_settings(&injected);
    assert_eq!(stripped["env"]["PATH"], "keep");
    assert!(stripped["env"].get("ANTHROPIC_BASE_URL").is_none());
    assert!(stripped.get("model").is_none());
}

#[test]
fn jsonc_parser_accepts_comments_and_trailing_commas() {
    let parsed = parse_json_or_jsonc(
        r#"{
          // OpenCode accepts this style.
          "provider": {
            "anthropic": {},
          },
        }"#,
    )
    .expect("jsonc should parse");

    assert!(parsed["provider"].get("anthropic").is_some());
}
