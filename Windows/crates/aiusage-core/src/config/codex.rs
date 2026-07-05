#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexManagedConfig<'a> {
    pub base_url: &'a str,
    pub bearer_token: &'a str,
    pub model: &'a str,
    pub global_toml: &'a str,
    pub node_toml: &'a str,
}

const HEADER_BEGIN: &str = "# >>> AIUSAGE-CODEX-PROXY BEGIN (managed, do not edit) >>>";
const HEADER_END: &str = "# <<< AIUSAGE-CODEX-PROXY END <<<";
const BASE_BEGIN: &str = "# >>> AIUSAGE-CODEX-BASE BEGIN (managed, do not edit) >>>";
const BASE_END: &str = "# <<< AIUSAGE-CODEX-BASE END <<<";
const PROVIDER_BEGIN: &str = "# >>> AIUSAGE-CODEX-PROVIDER BEGIN (managed, do not edit) >>>";
const PROVIDER_END: &str = "# <<< AIUSAGE-CODEX-PROVIDER END <<<";
const PROVIDER_ID: &str = "aiusage-proxy";

pub fn codex_provider_id() -> &'static str {
    PROVIDER_ID
}

pub fn strip_codex_managed_blocks(content: &str) -> String {
    let begins = [HEADER_BEGIN, BASE_BEGIN, PROVIDER_BEGIN];
    let ends = [HEADER_END, BASE_END, PROVIDER_END];
    let mut result = Vec::new();
    let mut skipping = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if begins.contains(&trimmed) {
            skipping = true;
            continue;
        }
        if ends.contains(&trimmed) {
            skipping = false;
            continue;
        }
        if !skipping {
            result.push(line);
        }
    }

    trim_outer_newlines(&result.join("\n"))
}

pub fn inject_codex_managed_config(original: &str, config: CodexManagedConfig<'_>) -> String {
    let clean = strip_codex_managed_blocks(original);
    let merged = merge_codex_base_fragments(config.global_toml, config.node_toml);
    let base_key_names: Vec<String> = merged
        .top_level
        .iter()
        .filter_map(|line| top_level_key_name(line))
        .collect();
    let base_table_headers: Vec<String> = merged
        .tables
        .iter()
        .filter_map(|block| first_table_header(block))
        .collect();

    let mut body = Vec::new();
    let mut seen_table = false;
    let mut skip_table = false;

    for line in clean.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            seen_table = true;
            let header = normalized_table_header(trimmed);
            skip_table = base_table_headers.contains(&header);
            if skip_table {
                continue;
            }
        } else if skip_table {
            continue;
        }

        if !seen_table {
            if is_top_level_key(trimmed, "model") || is_top_level_key(trimmed, "model_provider") {
                continue;
            }
            if let Some(key) = top_level_key_name(trimmed) {
                if base_key_names.contains(&key) {
                    continue;
                }
            }
        }
        body.push(line);
    }

    let mut output = vec![
        HEADER_BEGIN.to_string(),
        format!("model = {}", toml_string(config.model)),
        format!("model_provider = {}", toml_string(PROVIDER_ID)),
        HEADER_END.to_string(),
    ];

    if !merged.top_level.is_empty() {
        output.push(String::new());
        output.push(BASE_BEGIN.to_string());
        output.extend(merged.top_level);
        output.push(BASE_END.to_string());
    }

    output.push(String::new());
    output.push(body.join("\n"));

    if !merged.tables.is_empty() {
        output.push(String::new());
        output.push(BASE_BEGIN.to_string());
        output.extend(merged.tables);
        output.push(BASE_END.to_string());
    }

    output.push(String::new());
    output.push(PROVIDER_BEGIN.to_string());
    output.push(format!("[model_providers.{PROVIDER_ID}]"));
    output.push(format!("name = {}", toml_string("AIUsage Proxy")));
    output.push(format!("base_url = {}", toml_string(config.base_url)));
    output.push(format!("wire_api = {}", toml_string("responses")));
    output.push(format!(
        "experimental_bearer_token = {}",
        toml_string(config.bearer_token)
    ));
    output.push(PROVIDER_END.to_string());
    output.push(String::new());

    output.join("\n")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexBaseMerge {
    pub top_level: Vec<String>,
    pub tables: Vec<String>,
}

pub fn merge_codex_base_fragments(global: &str, node: &str) -> CodexBaseMerge {
    let global = split_codex_fragment(global);
    let node = split_codex_fragment(node);

    let mut top_order: Vec<String> = Vec::new();
    let mut top_map = std::collections::BTreeMap::<String, String>::new();
    for (key, line) in global.top.into_iter().chain(node.top) {
        if !top_map.contains_key(&key) {
            top_order.push(key.clone());
        }
        top_map.insert(key, line);
    }

    let mut table_order: Vec<String> = Vec::new();
    let mut table_map = std::collections::BTreeMap::<String, String>::new();
    for (header, block) in global.tables.into_iter().chain(node.tables) {
        if !table_map.contains_key(&header) {
            table_order.push(header.clone());
        }
        table_map.insert(header, block);
    }

    CodexBaseMerge {
        top_level: top_order
            .into_iter()
            .filter_map(|key| top_map.remove(&key))
            .collect(),
        tables: table_order
            .into_iter()
            .filter_map(|header| table_map.remove(&header))
            .collect(),
    }
}

#[derive(Default)]
struct FragmentParts {
    top: Vec<(String, String)>,
    tables: Vec<(String, String)>,
}

fn split_codex_fragment(fragment: &str) -> FragmentParts {
    let mut parts = FragmentParts::default();
    let mut current_header: Option<String> = None;
    let mut current_block: Vec<String> = Vec::new();

    fn flush(
        parts: &mut FragmentParts,
        current_header: &mut Option<String>,
        current_block: &mut Vec<String>,
    ) {
        if let Some(header) = current_header.take() {
            parts.tables.push((header, current_block.join("\n")));
        }
        current_block.clear();
    }

    for line in fragment.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            flush(&mut parts, &mut current_header, &mut current_block);
            current_header = Some(normalized_table_header(trimmed));
            current_block.push(line.to_string());
        } else if current_header.is_some() {
            current_block.push(line.to_string());
        } else if let Some(key) = top_level_key_name(trimmed) {
            parts.top.push((key, line.to_string()));
        }
    }
    flush(&mut parts, &mut current_header, &mut current_block);
    parts
}

fn top_level_key_name(trimmed: &str) -> Option<String> {
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    let (key, _) = trimmed.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_string())
}

fn normalized_table_header(trimmed: &str) -> String {
    trimmed
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim()
        .to_string()
}

fn first_table_header(block: &str) -> Option<String> {
    block
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with('['))
        .map(normalized_table_header)
}

fn is_top_level_key(trimmed: &str, key: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix(key) else {
        return false;
    };
    rest.trim_start().starts_with('=')
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn trim_outer_newlines(value: &str) -> String {
    value.trim_matches('\n').to_string()
}
