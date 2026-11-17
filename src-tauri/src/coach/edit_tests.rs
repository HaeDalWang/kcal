//! `edit.rs` 테스트.

use super::*;

const CONFIG: &str = "# top comment\nmodel = \"x\"\n\n[plugins.\"a@m\"]\nenabled = true # keep\n\n[plugins.\"b@m\"]\n\n[mcp_servers.one]\ncommand = \"x\"\nargs = [\n  [\"not\", \"a header\"],\n  \"y\",\n]\n[mcp_servers.one.env]\nenabled = \"leave me\"\n";

/// 바뀐 줄 번호 하나만 다르고 나머지 줄이 같은지.
fn only_line_differs(before: &str, after: &str, line: usize) {
    let (a, b): (Vec<&str>, Vec<&str>) =
        (before.split('\n').collect(), after.split('\n').collect());
    assert_eq!(a.len(), b.len());
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if i != line {
            assert_eq!(x, y, "line {i} changed");
        }
    }
}

#[test]
fn toml_replaces_existing_enabled_value_and_keeps_comment() {
    let edit = set_toml_enabled(CONFIG, "plugins.\"a@m\"", false).unwrap();
    assert!(edit.changed);
    assert_eq!(edit.before.as_deref(), Some("enabled = true # keep"));
    assert_eq!(edit.after, "enabled = false # keep");
    only_line_differs(CONFIG, &edit.content, 4);
}

#[test]
fn toml_inserts_enabled_right_below_header_when_missing() {
    let edit = set_toml_enabled(CONFIG, "plugins.\"b@m\"", true).unwrap();
    assert_eq!(edit.before, None);
    let expected = CONFIG.replace("[plugins.\"b@m\"]\n", "[plugins.\"b@m\"]\nenabled = true\n");
    assert_eq!(edit.content, expected);
}

#[test]
fn toml_rejects_missing_or_duplicate_section() {
    assert!(
        set_toml_enabled(CONFIG, "plugins.\"zzz@m\"", false)
            .unwrap_err()
            .contains("없어")
    );
    let twice = format!("{CONFIG}[plugins.\"a@m\"]\nenabled = true\n");
    assert!(
        set_toml_enabled(&twice, "plugins.\"a@m\"", false)
            .unwrap_err()
            .contains("두 번")
    );
}

#[test]
fn toml_ignores_brackets_inside_multiline_array() {
    // 배열 안 `[\"not\", ...]` 줄을 헤더로 보면 section이 거기서 끝나 `.env` 앞에 넣을 자리가 틀어진다.
    let edit = set_toml_enabled(CONFIG, "mcp_servers.one", false).unwrap();
    let expected = CONFIG.replace(
        "[mcp_servers.one]\n",
        "[mcp_servers.one]\nenabled = false\n",
    );
    assert_eq!(edit.content, expected);
    let lines: Vec<&str> = CONFIG.split('\n').collect();
    assert_eq!(header_indices(&lines).len(), 4);
}

#[test]
fn toml_does_not_touch_keys_of_env_subsection() {
    let edit = set_toml_enabled(CONFIG, "mcp_servers.one", false).unwrap();
    assert!(
        edit.content
            .contains("[mcp_servers.one.env]\nenabled = \"leave me\"\n")
    );
    // 하위 section 자체를 고치려 하면 값이 bool이 아니라 거부한다.
    assert!(set_toml_enabled(CONFIG, "mcp_servers.one.env", false).is_err());
}

#[test]
fn toml_keeps_every_other_byte_and_reports_no_change_when_already_set() {
    let edit = set_toml_enabled(CONFIG, "plugins.\"a@m\"", true).unwrap();
    assert!(!edit.changed);
    assert_eq!(edit.content, CONFIG);
    let crlf = CONFIG.replace('\n', "\r\n");
    let edit = set_toml_enabled(&crlf, "plugins.\"b@m\"", true).unwrap();
    assert_eq!(
        edit.content,
        crlf.replace(
            "[plugins.\"b@m\"]\r\n",
            "[plugins.\"b@m\"]\r\nenabled = true\r\n"
        )
    );
}

const KIRO: &str = "{\n  \"mcpServers\": {\n    \"zeta\": {\n      \"command\": \"z\"\n    },\n    \"alpha\": {\n      \"command\": \"a\",\n      \"disabled\": false\n    }\n  },\n  \"other\": 1\n}\n";

#[test]
fn json_sets_kiro_disabled_keeping_key_order_and_trailing_newline() {
    let edit = set_json_server_disabled(KIRO, "zeta", true).unwrap();
    assert!(edit.changed);
    assert_eq!(
        edit.content,
        "{\n  \"mcpServers\": {\n    \"zeta\": {\n      \"command\": \"z\",\n      \"disabled\": true\n    },\n    \"alpha\": {\n      \"command\": \"a\",\n      \"disabled\": false\n    }\n  },\n  \"other\": 1\n}\n"
    );
    let no_newline = KIRO.trim_end();
    assert!(
        !set_json_server_disabled(no_newline, "alpha", true)
            .unwrap()
            .content
            .ends_with('\n')
    );
    assert!(
        !set_json_server_disabled(KIRO, "alpha", false)
            .unwrap()
            .changed
    );
    assert!(
        !set_json_server_disabled(KIRO, "zeta", false)
            .unwrap()
            .changed
    );
}

#[test]
fn json_sets_claude_enabled_plugins_entry() {
    let settings = "{\n  \"model\": \"x\",\n  \"enabledPlugins\": {\n    \"b@m\": true,\n    \"a@m\": false\n  }\n}\n";
    let edit = set_json_plugin_enabled(settings, "b@m", false).unwrap();
    assert_eq!(
        edit.content,
        settings.replace("\"b@m\": true", "\"b@m\": false")
    );
    assert_eq!(edit.before.as_deref(), Some("\"b@m\": true"));
    assert!(set_json_plugin_enabled(settings, "new@m", true).is_err());
}

#[test]
fn json_rejects_jsonc_and_invalid_json() {
    let jsonc = "{\n  // comment\n  \"mcpServers\": {}\n}\n";
    assert!(
        set_json_server_disabled(jsonc, "x", true)
            .unwrap_err()
            .contains("JSON")
    );
    let trailing = "{\"enabledPlugins\": {\"a@m\": true,}}";
    assert!(set_json_plugin_enabled(trailing, "a@m", false).is_err());
    assert!(set_json_plugin_enabled("[]", "a@m", false).is_err());
}

#[test]
fn json_rejects_names_not_in_the_file() {
    assert!(
        set_json_server_disabled(KIRO, "missing", true)
            .unwrap_err()
            .contains("missing")
    );
}

const SETTINGS: &str = "{\n  \"env\": {\n    \"A\": \"1\"\n  },\n  \"effortLevel\": \"high\",\n  \"modelSettings\": {\n    \"claude-opus-5-5\": {\n      \"effortLevel\": \"xhigh\"\n    }\n  },\n  \"hooks\": {\n    \"SessionStart\": [\n      {\n        \"hooks\": [\n          {\n            \"command\": \"a\"\n          },\n          {\n            \"command\": \"b\"\n          }\n        ]\n      }\n    ]\n  }\n}\n";

fn path(segs: &[&str]) -> Vec<Seg> {
    segs.iter()
        .map(|s| s.parse().map_or_else(|_| key(s), Seg::Index))
        .collect()
}

#[test]
fn json_ops_remove_and_set_nested_keys_keeping_order_and_newline() {
    let ops = [
        JsonOp::Remove(path(&["effortLevel"])),
        JsonOp::Set(
            path(&["modelSettings", "claude-opus-5", "effortLevel"]),
            Value::from("high"),
        ),
    ];
    let edit = apply_json_ops(SETTINGS, &ops).unwrap();
    let expected = SETTINGS
        .replace("  \"effortLevel\": \"high\",\n", "")
        .replace(
            "      \"effortLevel\": \"xhigh\"\n    }\n  },",
            "      \"effortLevel\": \"xhigh\"\n    },\n    \"claude-opus-5\": {\n      \"effortLevel\": \"high\"\n    }\n  },",
        );
    assert_eq!(edit.content, expected);
    assert_eq!(
        edit.changes,
        [
            JsonChange {
                location: "effortLevel".into(),
                before: Some("\"high\"".into()),
                after: None
            },
            JsonChange {
                location: "modelSettings.\"claude-opus-5\".effortLevel".into(),
                before: None,
                after: Some("\"high\"".into())
            },
        ]
    );
}

#[test]
fn json_ops_remove_array_item_and_skip_set_to_same_value() {
    let ops = [JsonOp::Remove(path(&[
        "hooks",
        "SessionStart",
        "0",
        "hooks",
        "0",
    ]))];
    let edit = apply_json_ops(SETTINGS, &ops).unwrap();
    assert!(!edit.content.contains("\"a\"") && edit.content.contains("\"b\""));
    assert_eq!(edit.changes[0].location, "hooks.SessionStart[0].hooks[0]");
    let same = [JsonOp::Set(path(&["env", "A"]), Value::from("1"))];
    let edit = apply_json_ops(SETTINGS, &same).unwrap();
    assert!(edit.changes.is_empty());
    assert_eq!(edit.content, SETTINGS);
}

#[test]
fn json_ops_refuse_missing_keys_blocked_paths_and_jsonc() {
    assert!(apply_json_ops(SETTINGS, &[JsonOp::Remove(path(&["nope"]))]).is_err());
    let blocked = [JsonOp::Set(path(&["effortLevel", "x"]), Value::from(1))];
    assert!(apply_json_ops(SETTINGS, &blocked).is_err());
    assert!(apply_json_ops("{\n  // c\n}\n", &[JsonOp::Remove(path(&["a"]))]).is_err());
}

#[test]
fn only_intended_rejects_changes_outside_the_edited_paths() {
    let original: Value = serde_json::from_str(SETTINGS).unwrap();
    let ops = [JsonOp::Remove(path(&["hooks", "SessionStart", "0"]))];
    assert!(only_intended(
        &original,
        &[path(&["hooks", "SessionStart"])],
        &ops
    ));
    assert!(!only_intended(&original, &[path(&["hooks"])], &ops));
    assert!(!only_intended(&original, &[path(&["env", "A"])], &ops));
    // Set은 없던 중간 객체만 새로 만들 수 있다. 있던 객체 전체가 바뀌면 거부.
    let set = [JsonOp::Set(
        path(&["skillOverrides", "x"]),
        Value::from("off"),
    )];
    assert!(only_intended(&original, &[path(&["skillOverrides"])], &set));
    let set = [JsonOp::Set(path(&["env", "B"]), Value::from("2"))];
    assert!(!only_intended(&original, &[path(&["env"])], &set));
}

#[test]
fn content_hash_is_fnv1a_64_hex() {
    // FNV-1a 64 공개 테스트 값.
    assert_eq!(content_hash(b""), "cbf29ce484222325");
    assert_eq!(content_hash(b"a"), "af63dc4c8601ec8c");
}
