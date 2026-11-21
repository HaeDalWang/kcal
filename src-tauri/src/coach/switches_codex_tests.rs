// switches::tests의 기존 HOME·컨텍스트 도우미를 재사용한다.
fn codex_evidence(home: &TempDir, calls: bool, sessions: usize) -> UsageIndex {
    let timestamp = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    for i in 0..sessions {
        let meta = serde_json::json!({"timestamp": timestamp, "type": "session_meta",
            "payload": {"cwd": home.path()}});
        let call = serde_json::json!({"timestamp": timestamp, "type": "event_msg", "payload": {
            "type": "item_completed", "item": {"type": "McpToolCall", "id": format!("call-{i}"),
            "server": "docs", "tool": "read"}}});
        home.write(
            &format!(".codex/sessions/rollout-{i}.jsonl"),
            &format!(
                "{meta}\n{}",
                if calls {
                    format!("{call}\n")
                } else {
                    String::new()
                }
            ),
        );
    }
    crate::coach::usage::read(
        home.path(),
        &Tuning::default(),
        crate::metrics::unix_secs(OffsetDateTime::now_utc()),
    )
}

fn codex_snapshot() -> ContextSnapshot {
    snapshot(vec![(
        Agent::Codex,
        vec![
            item(ContextKind::Mcp, "docs", Some(500), Loading::Startup),
            item(ContextKind::Plugins, "big@m", Some(900), Loading::Startup),
        ],
    )])
}

#[test]
fn proposes_codex_mcp_only_with_enough_complete_zero_use_sessions() {
    let tuning = Tuning::default();
    for (name, sessions, used, partial, expected) in [
        ("idle", tuning.diagnose_min_sessions, false, false, true),
        ("used", tuning.diagnose_min_sessions, true, false, false),
        ("few", tuning.diagnose_min_sessions - 1, false, false, false),
        ("partial", tuning.diagnose_min_sessions, false, true, false),
    ] {
        let home = home(&format!("codex-off-{name}"));
        let mut evidence = codex_evidence(&home, used, sessions);
        evidence.codex.as_mut().unwrap().is_partial = partial;
        let all = list(
            home.path(),
            Some(&codex_snapshot()),
            Some(&evidence),
            &tuning,
        );
        let server = find(all.clone(), "codex/mcp:docs").unwrap();
        assert_eq!(server.is_proposed, expected, "{name}");
        let usage = server.usage.unwrap();
        assert_eq!(usage.sessions, sessions);
        assert_eq!(usage.total > 0, used);
        assert!(
            all.iter()
                .filter(|s| s.kind == SwitchKind::Plugin && s.agent == Agent::Codex
                    || s.agent == Agent::Kiro)
                .all(|s| !s.is_proposed && s.usage.is_none())
        );
    }
}

#[test]
fn codex_off_proposal_uses_existing_apply_and_undo_path() {
    let home = home("codex-off-undo");
    let evidence = codex_evidence(&home, false, Tuning::default().diagnose_min_sessions);
    let config = home.path().join(".codex/config.toml");
    let before = std::fs::read(&config).unwrap();
    let context = codex_snapshot();
    let tuning = Tuning::default();
    let preview = crate::coach::preview(
        home.path(),
        Some(&context),
        Some(&evidence),
        &tuning,
        "codex/mcp:docs",
        false,
    )
    .unwrap();
    let log = home.path().join("app-data/coach-log.jsonl");
    let applied = crate::coach::apply(
        home.path(),
        Some(&context),
        Some(&evidence),
        &tuning,
        &log,
        "codex/mcp:docs",
        false,
        &preview.before_hash,
        OffsetDateTime::now_utc(),
    )
    .unwrap();
    assert!(
        !find(
            list(home.path(), Some(&context), Some(&evidence), &tuning),
            "codex/mcp:docs"
        )
        .unwrap()
        .enabled
    );
    crate::coach::undo(home.path(), &log, &applied.id, OffsetDateTime::now_utc()).unwrap();
    assert_eq!(std::fs::read(config).unwrap(), before);
}
