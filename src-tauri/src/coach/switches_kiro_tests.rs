#[test]
fn proposes_kiro_mcp_only_with_enough_complete_zero_use_sessions() {
    let tuning = Tuning::default();
    for (name, sessions, used, partial, enabled, tokens, expected) in [
        ("idle", tuning.diagnose_min_sessions, false, false, true, 500, true),
        ("few", tuning.diagnose_min_sessions - 1, false, false, true, 500, false),
        ("partial", tuning.diagnose_min_sessions, false, true, true, 500, false),
        ("used", tuning.diagnose_min_sessions, true, false, true, 500, false),
        ("off", tuning.diagnose_min_sessions, false, false, false, 500, false),
        ("free", tuning.diagnose_min_sessions, false, false, true, 0, false),
    ] {
        let home = home(&format!("kiro-off-{name}"));
        home.write("work/app/exists", "");
        home.write(".kiro/settings/mcp.json", &serde_json::json!({"mcpServers":{"k":{"command":"node","disabled":!enabled}}}).to_string());
        let timestamp = OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap();
        for i in 0..sessions {
            home.write(&format!(".kiro/sessions/h/s{i}/session.json"), &serde_json::json!({"workspacePaths":[home.path().join("work/app")]}).to_string());
            let payload = if used { serde_json::json!({"type":"tool_call","toolCallId":format!("c{i}"),"_meta":{"kiro":{"serverName":"k"}}}) } else { serde_json::json!({"type":"session_start"}) };
            home.write(&format!(".kiro/sessions/h/s{i}/messages.jsonl"), &serde_json::json!({"timestamp":timestamp,"payload":payload}).to_string());
        }
        let mut evidence = crate::coach::usage::read(home.path(), &tuning, crate::metrics::unix_secs(OffsetDateTime::now_utc()));
        evidence.kiro.as_mut().unwrap().is_partial = partial;
        let context = snapshot(vec![(Agent::Kiro, vec![item(ContextKind::Mcp,"k",Some(tokens),Loading::Startup)])]);
        let server = find(list(home.path(),Some(&context),Some(&evidence),&tuning),"kiro/mcp:k").unwrap();
        assert_eq!(server.is_proposed, expected, "{name}");
        let usage = server.usage.as_ref().unwrap();
        assert_eq!(usage.sessions, sessions);
        assert_eq!(usage.total, if used { sessions as u32 } else { 0 });
        if used {
            assert!(server.scope_guide.is_some());
            let before = std::fs::read(home.path().join(".kiro/settings/mcp.json")).unwrap();
            assert!(crate::coach::scope::preview(&server).is_err());
            assert!(crate::coach::scope::apply(&server, &home.path().join("log"), "ignored", OffsetDateTime::now_utc()).is_err());
            assert!(!home.path().join("work/app/.claude").exists());
            assert!(!home.path().join("work/app/.kiro").exists());
            assert_eq!(std::fs::read(home.path().join(".kiro/settings/mcp.json")).unwrap(), before);
        }
    }
}
