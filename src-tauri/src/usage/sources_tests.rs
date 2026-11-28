//! `sources.rs` 테스트.

use super::*;
use std::io::Write;
use std::net::TcpListener;
use time::macros::datetime;

const TOKEN: &str = "tok-SECRET-4f2a9c";
const ACCOUNT: &str = "acct-SECRET-77b1";

/// 응답 하나를 돌려주는 로컬 HTTP 서버. 받은 요청 전체(헤더 포함)를 본문에 되돌려 보낸다.
fn serve_once(status_line: &'static str, extra_headers: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/usage", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
        }
        let body = format!(
            "{{\"echo\": {:?}, \"token\": \"{TOKEN}\"}}",
            String::from_utf8_lossy(&request)
        );
        let response = format!(
            "{status_line}\r\n{extra_headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    url
}

fn temp_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("kcal-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let auth = format!(r#"{{"tokens": {{"access_token": "{TOKEN}", "account_id": "{ACCOUNT}"}}}}"#);
    std::fs::write(home.join(".codex/auth.json"), auth).unwrap();
    home
}

fn assert_no_secret(text: &str) {
    assert!(
        !text.contains(TOKEN) && !text.contains(ACCOUNT),
        "secret leaked: {text}"
    );
}

#[test]
fn failure_reason_never_contains_token_even_when_server_echoes_it() {
    let home = temp_home("leak");
    let url = serve_once("HTTP/1.1 401 Unauthorized", "");
    let ctx = Context::with_urls(home.clone(), String::new(), url);
    let outcome =
        tauri::async_runtime::block_on(codex(&ctx, OffsetDateTime::now_utc(), &Tuning::default()));
    let reason = outcome.usage.reason.clone().unwrap();
    // rollout이 하나도 없으면 대체 기록 이야기는 붙이지 않는다.
    assert_eq!(
        reason,
        "Codex 로그인이 만료됐어요. codex를 한 번 실행한 뒤 새로고침하세요."
    );
    assert_no_secret(&reason);
    assert_no_secret(&format!("{outcome:?}"));
    assert_no_secret(&serde_json::to_string(&outcome.usage).unwrap());
    let _ = std::fs::remove_dir_all(home);
}

/// 병렬 테스트가 같은 임시 폴더를 쓰지 않게 하는 번호.
fn unique() -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// 한 번 요청한 Codex 실패 이유(영어). 상태 번호가 화면에 나오면 안 된다.
fn codex_reason_en(url: String, tuning: &Tuning) -> (String, ReasonKind) {
    let home = temp_home(&format!("reason-en-{}", unique()));
    let ctx = Context::with_urls(home.clone(), String::new(), url);
    let outcome = crate::i18n::with_lang(crate::i18n::Lang::En, || {
        tauri::async_runtime::block_on(codex(&ctx, OffsetDateTime::now_utc(), tuning))
    });
    let _ = std::fs::remove_dir_all(home);
    (
        outcome.usage.reason.unwrap(),
        outcome.usage.reason_kind.unwrap(),
    )
}

#[test]
fn http_401_and_403_say_login_expired_without_status_code() {
    for line in ["HTTP/1.1 401 Unauthorized", "HTTP/1.1 403 Forbidden"] {
        let (reason, kind) = codex_reason_en(serve_once(line, ""), &Tuning::default());
        // 다시 로그인하면 풀리는 일이라 고장이 아니다.
        assert_eq!(kind, ReasonKind::Setup);
        assert_eq!(
            reason,
            "Your Codex login expired. Run codex once, then press Refresh."
        );
    }
}

#[test]
fn http_5xx_says_service_has_trouble_without_status_code() {
    let (reason, kind) = codex_reason_en(
        serve_once("HTTP/1.1 503 Service Unavailable", ""),
        &Tuning::default(),
    );
    assert_eq!(kind, ReasonKind::Failure);
    assert_eq!(
        reason,
        "Codex is having trouble right now. kcal retries on the next check."
    );
    let (reason, _) = codex_reason_en(
        serve_once("HTTP/1.1 418 I'm a teapot", ""),
        &Tuning::default(),
    );
    assert_eq!(
        reason,
        "Codex sent an unexpected answer. kcal retries on the next check."
    );
}

#[test]
fn unreachable_server_says_cannot_reach_and_retries() {
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/usage", listener.local_addr().unwrap())
    };
    let (reason, kind) = codex_reason_en(closed, &Tuning::default());
    assert_eq!(kind, ReasonKind::Failure);
    assert_eq!(
        reason,
        "Can't reach Codex. Check your connection; kcal retries on the next check."
    );
}

#[test]
fn silent_server_times_out_with_the_same_cannot_reach_message() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/usage", listener.local_addr().unwrap());
    let tuning = Tuning {
        http_timeout_secs: 1,
        ..Tuning::default()
    };
    let (reason, kind) = codex_reason_en(url, &tuning);
    drop(listener);
    assert_eq!(kind, ReasonKind::Failure);
    assert_eq!(
        reason,
        "Can't reach Codex. Check your connection; kcal retries on the next check."
    );
}

#[cfg(all(unix, target_os = "macos"))]
fn fake_security(name: &str, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "kcal-test-{name}-{}-{}",
        std::process::id(),
        unique()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let program = dir.join("security");
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

#[cfg(all(unix, target_os = "macos"))]
fn keychain_reason_en(script: &str, timeout: Duration) -> String {
    let program = fake_security("keychain", script);
    let result = crate::i18n::with_lang(crate::i18n::Lang::En, || {
        tauri::async_runtime::block_on(read_keychain(&program, "svc", timeout))
    });
    let _ = std::fs::remove_dir_all(program.parent().unwrap());
    let Err(Fail(reason, _)) = result else {
        panic!("keychain read unexpectedly succeeded")
    };
    reason
}

#[cfg(all(unix, target_os = "macos"))]
#[test]
fn keychain_timeout_tells_the_user_to_click_allow() {
    let reason = keychain_reason_en("#!/bin/sh\nexec sleep 30\n", Duration::from_secs(1));
    assert_eq!(
        reason,
        "macOS may be asking whether kcal can read your Claude Code login. Click Allow, then press Refresh."
    );
}

#[cfg(all(unix, target_os = "macos"))]
#[test]
fn keychain_item_not_found_says_sign_in() {
    let reason = keychain_reason_en("#!/bin/sh\nexit 44\n", Duration::from_secs(5));
    assert_eq!(
        reason,
        "No Claude subscription login found. With an API key, Bedrock or Vertex there is no limit to show; Startup context and Checks still work. On Pro or Max, run claude and sign in, then press Refresh."
    );
}

#[cfg(all(unix, target_os = "macos"))]
#[test]
fn keychain_other_failure_mentions_allow_and_sign_in() {
    let reason = keychain_reason_en("#!/bin/sh\nexit 51\n", Duration::from_secs(5));
    assert_eq!(
        reason,
        "Can't read your Claude Code login. If macOS asks, click Allow; otherwise run claude and sign in, then press Refresh. If you use an API key, Bedrock or Vertex, there is no limit to show."
    );
}

#[cfg(all(unix, target_os = "macos"))]
#[test]
fn keychain_success_returns_the_secret() {
    let program = fake_security("keychain-ok", "#!/bin/sh\nprintf 'secret'\n");
    let secret =
        tauri::async_runtime::block_on(read_keychain(&program, "svc", Duration::from_secs(5)));
    let _ = std::fs::remove_dir_all(program.parent().unwrap());
    assert_eq!(secret.unwrap().0, "secret");
}

#[test]
fn rate_limited_api_reports_retry_after_and_falls_back() {
    let home = temp_home("429");
    let url = serve_once("HTTP/1.1 429 Too Many Requests", "Retry-After: 120\r\n");
    let ctx = Context::with_urls(home.clone(), String::new(), url);
    let outcome =
        tauri::async_runtime::block_on(codex(&ctx, OffsetDateTime::now_utc(), &Tuning::default()));
    assert_eq!(
        outcome.rate_limit,
        Some(RateLimit {
            retry_after_secs: Some(120)
        })
    );
    assert_eq!(outcome.usage.source, None);
    assert_eq!(
        outcome.usage.reason.as_deref(),
        Some("요청이 몰려 잠시 막혔어요. kcal이 다음 확인 때 다시 시도해요.")
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn codex_falls_back_to_newest_rollout_rate_limits() {
    let home = temp_home("rollout");
    let day = home.join(".codex/sessions/2026/11/04");
    std::fs::create_dir_all(&day).unwrap();
    let line = |ts: &str, pct: f64| {
        format!(
            r#"{{"timestamp":"{ts}","payload":{{"type":"token_count","rate_limits":{{"primary":{{"used_percent":{pct},"resets_at":1893456000}},"secondary":{{"used_percent":1.0,"resets_at":1893456000}}}}}}}}"#
        )
    };
    let rows = [
        line("2026-11-04T10:00:00Z", 5.0),
        "{\"not\":\"limits\"}".into(),
        line("2026-11-04T11:00:00Z", 8.0),
    ];
    std::fs::write(day.join("rollout-a.jsonl"), rows.join("\n")).unwrap();
    let reading = codex_from_log(&home, datetime!(2026-11-04 12:00 UTC), 3)
        .unwrap()
        .unwrap();
    assert_eq!(reading.observed_at, Some(datetime!(2026-11-04 11:00 UTC)));
    assert_eq!(reading.windows[0].used_percent, 8.0);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn codex_log_fallback_checks_only_as_many_rollouts_as_configured() {
    let home = temp_home("rollout-limit");
    let day = home.join(".codex/sessions/2026/11/04");
    std::fs::create_dir_all(&day).unwrap();
    let limits = r#"{"timestamp":"2026-11-04T10:00:00Z","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":5.0,"resets_at":1893456000},"secondary":{"used_percent":1.0,"resets_at":1893456000}}}}"#;
    let (older, newer) = (day.join("rollout-old.jsonl"), day.join("rollout-new.jsonl"));
    std::fs::write(&older, limits).unwrap();
    std::fs::write(&newer, "{\"not\":\"limits\"}").unwrap();
    for (path, secs) in [(&older, 1_000), (&newer, 2_000)] {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }
    let now = datetime!(2026-11-04 12:00 UTC);
    assert!(codex_from_log(&home, now, 1).is_err());
    assert!(codex_from_log(&home, now, 2).is_ok());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn claude_token_errors_do_not_contain_token() {
    let now = datetime!(2026-11-04 12:00 UTC);
    let expired =
        format!(r#"{{"claudeAiOauth": {{"accessToken": "{TOKEN}", "expiresAt": 1000}}}}"#);
    let Err(Fail(reason, _)) = claude_token(&expired, now) else {
        panic!("expired token accepted")
    };
    assert_eq!(
        reason,
        "Claude Code 로그인이 만료됐어요. claude를 한 번 실행한 뒤 새로고침하세요."
    );
    let valid = format!(r#"{{"claudeAiOauth": {{"accessToken": "{TOKEN}", "expiresAt": 9e15}}}}"#);
    let token = claude_token(&valid, now).unwrap();
    assert_no_secret(&format!("{token:?}"));
    let Err(Fail(reason, _)) = claude_token(&format!("{TOKEN} not json"), now) else {
        panic!()
    };
    assert_no_secret(&reason);
}

fn claude_log_home(name: &str, rows: &[String]) -> PathBuf {
    let home = temp_home(name);
    let log = paths::claude_statusline_log(&home);
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(log, rows.join("\n")).unwrap();
    home
}

fn claude_row(ts: u64, pct: f64) -> String {
    format!(
        r#"{{"ts":{ts},"five_hour":{{"pct":{pct},"resets_at":1893456000}},"seven_day":{{"pct":1.0,"resets_at":1893456000}}}}"#
    )
}

const LOG_NOW: OffsetDateTime = datetime!(2026-11-04 12:00 UTC);

#[test]
fn claude_log_fallback_reads_latest_statusline_record() {
    let rows = [
        claude_row(1_793_870_000, 5.0),
        claude_row(1_793_880_000, 8.0),
    ];
    let home = claude_log_home("claude-log", &rows);
    let reading = claude_from_log(&home, LOG_NOW, 1 << 20).unwrap().unwrap();
    assert_eq!(reading.windows[0].used_percent, 8.0);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn claude_log_fallback_ignores_records_older_than_tail_range() {
    let old = claude_row(1_793_870_000, 5.0);
    let padding = "x".repeat(300);
    let home = claude_log_home("claude-tail", &[old.clone(), padding]);
    let Err(Fail(reason, _)) = claude_from_log(&home, LOG_NOW, 100) else {
        panic!("record outside the tail was read")
    };
    assert_eq!(reason, "statusLine 기록에 rate limit 줄 없음");
    // 범위를 넓히면 같은 기록이 읽힌다(범위 때문에 못 읽은 것임을 확인).
    assert!(claude_from_log(&home, LOG_NOW, 1 << 20).is_ok());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn claude_log_fallback_skips_malformed_lines() {
    let rows = [
        claude_row(1_793_880_000, 8.0),
        "{\"ts\": 1, \"five_hour\": {".into(),
        "not json".into(),
    ];
    let home = claude_log_home("claude-bad-line", &rows);
    let reading = claude_from_log(&home, LOG_NOW, 1 << 20).unwrap().unwrap();
    assert_eq!(reading.windows[0].used_percent, 8.0);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn expired_claude_token_fails_without_a_refresh_path() {
    // 갱신하려면 refreshToken을 읽어야 한다. 만료 판정은 네트워크 없는 순수 함수이고, 만료되면 바로 실패한다.
    let now = datetime!(2026-11-04 12:00 UTC);
    let at_ms = now.unix_timestamp() * 1000;
    let creds = format!(
        r#"{{"claudeAiOauth": {{"accessToken": "{TOKEN}", "refreshToken": "{ACCOUNT}", "expiresAt": {at_ms}}}}}"#
    );
    let Err(Fail(reason, _)) = claude_token(&creds, now) else {
        panic!("token expiring now was accepted")
    };
    assert_eq!(
        reason,
        "Claude Code 로그인이 만료됐어요. claude를 한 번 실행한 뒤 새로고침하세요."
    );
    assert_no_secret(&reason);
}

#[test]
fn changed_api_format_falls_back_to_log_instead_of_showing_zero_percent() {
    let home = temp_home("format-change");
    let day = home.join(".codex/sessions/2026/11/04");
    std::fs::create_dir_all(&day).unwrap();
    let limits = r#"{"timestamp":"2026-11-04T11:00:00Z","payload":{"rate_limits":{"primary":{"used_percent":8.0,"resets_at":1893456000},"secondary":{"used_percent":1.0,"resets_at":1893456000}}}}"#;
    std::fs::write(day.join("rollout-a.jsonl"), limits).unwrap();
    let url = serve_once("HTTP/1.1 200 OK", "");
    let ctx = Context::with_urls(home.clone(), String::new(), url);
    let outcome =
        tauri::async_runtime::block_on(codex(&ctx, OffsetDateTime::now_utc(), &Tuning::default()));
    assert_eq!(outcome.usage.source, Some(UsageSource::Log));
    assert_eq!(outcome.usage.windows[0].used_percent, 8.0);
    let reason = outcome.usage.reason.unwrap();
    assert_eq!(reason, "rate_limit 없음");
    assert_no_secret(&reason);
    let _ = std::fs::remove_dir_all(home);
}

/// 빈 홈: 설치 여부는 설정 폴더(·kiro-cli)로 정한다.
#[test]
fn detects_installed_agents_from_their_folders_and_kiro_cli() {
    let home = std::env::temp_dir().join(format!("kcal-test-installed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    for agent in Agent::ALL {
        assert!(!is_installed(agent, &home, None), "{agent:?}");
    }
    std::fs::write(home.join(".claude.json"), "{}").unwrap();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    assert!(is_installed(Agent::Claude, &home, None));
    assert!(is_installed(Agent::Codex, &home, None));
    assert!(!is_installed(Agent::Kiro, &home, None));
    let _ = std::fs::remove_dir_all(home);
    #[cfg(unix)]
    {
        let home = fake_kiro_home("kiro-installed", "#!/bin/sh\n");
        assert!(is_installed(Agent::Kiro, &home, None));
        let _ = std::fs::remove_dir_all(home);
    }
}

#[test]
fn codex_without_codex_folder_says_not_installed_in_plain_english() {
    let home = std::env::temp_dir().join(format!("kcal-test-no-codex-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let ctx = Context::with_urls(home.clone(), String::new(), String::new());
    let outcome = crate::i18n::with_lang(crate::i18n::Lang::En, || {
        tauri::async_runtime::block_on(codex(&ctx, OffsetDateTime::now_utc(), &Tuning::default()))
    });
    assert_eq!(
        outcome.usage.reason.as_deref(),
        Some("Codex isn't installed.")
    );
    assert_eq!(outcome.usage.reason_kind, Some(ReasonKind::Setup));
    assert!(outcome.usage.checked_at.is_some());
    // 폴더는 있는데 로그인 파일이 없으면 로그인 안내.
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let outcome = crate::i18n::with_lang(crate::i18n::Lang::En, || {
        tauri::async_runtime::block_on(codex(&ctx, OffsetDateTime::now_utc(), &Tuning::default()))
    });
    assert_eq!(
        outcome.usage.reason.as_deref(),
        Some("Can't read your Codex login. Run codex and sign in, then press Refresh.")
    );
    assert_eq!(outcome.usage.reason_kind, Some(ReasonKind::Setup));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn missing_statusline_log_is_not_a_reason() {
    let home = std::env::temp_dir().join(format!("kcal-test-no-log-{}", std::process::id()));
    assert!(matches!(claude_from_log(&home, LOG_NOW, 1 << 20), Ok(None)));
}

#[cfg(unix)]
fn fake_kiro_home(name: &str, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let home = temp_home(name);
    let cli = home.join(".local/bin/kiro-cli");
    std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
    std::fs::write(&cli, script).unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    home
}

#[cfg(unix)]
fn kiro_reason(name: &str, script: &str) -> (String, ReasonKind) {
    let home = fake_kiro_home(name, script);
    let ctx = Context::with_urls(home.clone(), String::new(), String::new());
    let outcome = crate::i18n::with_lang(crate::i18n::Lang::En, || {
        tauri::async_runtime::block_on(kiro(&ctx, OffsetDateTime::now_utc(), &Tuning::default()))
    });
    let _ = std::fs::remove_dir_all(home);
    (
        outcome.usage.reason.expect("kiro should fail"),
        outcome.usage.reason_kind.expect("kind goes with reason"),
    )
}

#[cfg(unix)]
#[test]
fn kiro_not_logged_in_says_sign_in_with_the_real_command() {
    let (reason, kind) = kiro_reason(
        "kiro-signed-out",
        "#!/bin/sh\necho 'Not logged in. Set the KIRO_API_KEY environment variable or run `kiro-cli login` first.' >&2\nexit 1\n",
    );
    assert_eq!(
        reason,
        "You're not signed in to Kiro. Run kiro-cli login, then press Refresh."
    );
    assert_eq!(kind, ReasonKind::Setup);
}

#[cfg(unix)]
#[test]
fn kiro_exit_zero_with_not_logged_in_text_also_says_sign_in() {
    let (reason, _) = kiro_reason("kiro-signed-out-zero", "#!/bin/sh\necho 'Not logged in'\n");
    assert!(
        reason.starts_with("You're not signed in to Kiro."),
        "{reason}"
    );
}

#[cfg(unix)]
#[test]
fn kiro_unreadable_output_does_not_claim_signed_out() {
    let (reason, kind) = kiro_reason("kiro-garbage", "#!/bin/sh\necho 'something new'\n");
    assert_eq!(
        reason,
        "Couldn't read Kiro usage; kcal retries on the next check."
    );
    assert_eq!(kind, ReasonKind::Failure);
}

#[cfg(unix)]
#[test]
fn kiro_failure_without_sign_in_text_keeps_the_generic_exit_message() {
    let (reason, _) = kiro_reason("kiro-exit2", "#!/bin/sh\necho boom >&2\nexit 2\n");
    assert!(
        reason.starts_with("Kiro CLI couldn't report your usage."),
        "{reason}"
    );
}

#[test]
fn missing_fallback_log_leaves_only_the_api_reason() {
    let home = std::env::temp_dir().join(format!("kcal-test-no-log2-{}", std::process::id()));
    let outcome = tauri::async_runtime::block_on(with_log_fallback(
        Agent::Claude,
        fail("api reason"),
        move || claude_from_log(&home, LOG_NOW, 1 << 20),
    ));
    assert_eq!(outcome.usage.reason.as_deref(), Some("api reason"));
    assert_eq!(outcome.usage.reason_kind, Some(ReasonKind::Failure));
}

#[cfg(unix)]
#[test]
fn kiro_timeout_reports_timeout_and_kills_the_process() {
    let script = "#!/bin/sh\necho $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 30\n";
    let home = fake_kiro_home("kiro-timeout", script);
    let pid_file = home.join(".local/bin/pid");
    let tuning = Tuning {
        kiro_timeout_secs: 1,
        ..Tuning::default()
    };
    let ctx = Context::with_urls(home.clone(), String::new(), String::new());
    let outcome = tauri::async_runtime::block_on(kiro(&ctx, OffsetDateTime::now_utc(), &tuning));
    assert_eq!(
        outcome.usage.reason.as_deref(),
        Some("Kiro가 제시간에 답하지 않았어요. kcal이 다음 확인 때 다시 시도해요.")
    );
    assert_eq!(outcome.usage.reason_kind, Some(ReasonKind::Failure));
    assert_eq!(outcome.usage.source, None);

    let pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .to_owned();
    let alive = || {
        std::process::Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    for _ in 0..40 {
        if !alive() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !alive(),
        "kiro-cli process {pid} is still running after timeout"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// 새 Mac(빈 HOME)에서 모든 에이전트가 값이나 이유 중 하나를 갖는지 본다. 실제 Keychain·PATH의 CLI는 건드리지 않는다:
/// - Codex: `codex()`를 그대로 부른다(파일만 읽는다).
/// - Kiro: `kiro()`는 실제 `PATH`를 읽어서, PATH에서 kiro-cli가 보이면 실제 CLI가 돌기 때문에 그때는 건너뛴다.
/// - Claude(macOS): `claude()`는 `/usr/bin/security`를 고정해서 실제 Keychain을 읽는다. 가짜 `security`(항목 없음, 44)로
///   `read_keychain`과 같은 대체 기록 경로를 조합해 본다. 다른 OS는 `claude()`를 그대로 부른다.
#[test]
fn fresh_home_usage_explains_each_agent() {
    use crate::i18n::{Lang, with_lang};
    let home = std::env::temp_dir().join(format!(
        "kcal-test-fresh-{}-{}",
        std::process::id(),
        unique()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let ctx = Context::with_urls(home.clone(), String::new(), String::new());
    let tuning = Tuning::default();
    let now = OffsetDateTime::now_utc();

    let path_env = std::env::var_os("PATH");
    let real_kiro = paths::kiro_cli_candidates(&home, path_env.as_deref())
        .iter()
        .any(|p| is_executable(p));

    #[cfg(target_os = "macos")]
    let claude_fresh = || {
        let program = fake_security("fresh", "#!/bin/sh\nexit 44\n");
        let api =
            tauri::async_runtime::block_on(read_keychain(&program, "svc", Duration::from_secs(5)))
                .map(|_| unreachable!("fake security finds no item"))
                .or_else(|e| {
                    if is_installed(Agent::Claude, &home, None) {
                        Err(e)
                    } else {
                        setup("claude() shows its not-found text here")
                    }
                });
        let log_home = home.clone();
        let out =
            tauri::async_runtime::block_on(with_log_fallback(Agent::Claude, api, move || {
                claude_from_log(&log_home, now, 1 << 20)
            }));
        let _ = std::fs::remove_dir_all(program.parent().unwrap());
        out
    };
    #[cfg(not(target_os = "macos"))]
    let claude_fresh = || tauri::async_runtime::block_on(claude(&ctx, now, &tuning));

    for lang in [Lang::Ko, Lang::En] {
        let mut outcomes = vec![
            with_lang(lang, || {
                tauri::async_runtime::block_on(codex(&ctx, now, &tuning))
            }),
            with_lang(lang, claude_fresh),
        ];
        if !real_kiro {
            outcomes.push(with_lang(lang, || {
                tauri::async_runtime::block_on(kiro(&ctx, now, &tuning))
            }));
        }
        for Outcome { usage, .. } in outcomes {
            let who = usage.agent.id();
            assert!(usage.checked_at.is_some(), "{who}: no checkedAt");
            let reason = usage.reason.as_deref().unwrap_or("");
            assert!(
                !usage.windows.is_empty() || !reason.is_empty(),
                "{who}: neither a value nor a reason"
            );
            // 빈 HOME에는 로그인도 설치도 없다: 사용자가 할 일(setup)이지 고장(failure)이 아니다.
            assert!(usage.windows.is_empty() && usage.source.is_none(), "{who}");
            assert_eq!(
                usage.reason_kind,
                Some(ReasonKind::Setup),
                "{who}: {reason}"
            );
        }
    }
    // 첫 조회 전(`get_usage`가 처음 돌려주는 값)은 이유도 checkedAt도 없다. 화면은 "첫 조회 중"으로 읽는다.
    let pending = crate::usage::UsageState::default().snapshot().unwrap();
    assert!(
        pending
            .agents
            .iter()
            .all(|a| a.checked_at.is_none() && a.reason.is_none())
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn claude_login_end_takes_precedence_over_access_expiry() {
    let now = datetime!(2026-11-28 12:00 UTC);
    let ms = now.unix_timestamp() * 1000;
    for access in [ms - 1, ms + 1000] {
        let creds = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"fixture","expiresAt":{access},"refreshTokenExpiresAt":{ms}}}}}"#
        );
        let Err(Fail(reason, kind)) = claude_token(&creds, now) else {
            panic!()
        };
        assert_eq!(kind, ReasonKind::Setup);
        assert_eq!(
            reason,
            "Claude 로그인 기간이 끝났어요. 터미널에서 claude를 실행해 /login으로 다시 로그인한 뒤 새로고침하세요."
        );
        let reason = crate::i18n::with_lang(crate::i18n::Lang::En, || {
            claude_token(&creds, now).unwrap_err().0
        });
        assert_eq!(
            reason,
            "Your Claude login period has ended. Run claude in a terminal, sign in again with /login, then press Refresh."
        );
    }
    let later = ms + 1000;
    let creds = format!(
        r#"{{"claudeAiOauth":{{"accessToken":"fixture","expiresAt":{ms},"refreshTokenExpiresAt":{later}}}}}"#
    );
    assert_eq!(
        claude_token(&creds, now).unwrap_err().0,
        login_expired(&CLAUDE)
    );
    let valid = r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#;
    assert_eq!(claude_token(valid, now).unwrap().1, None);
    let valid = format!(
        r#"{{"claudeAiOauth":{{"accessToken":"fixture","refreshTokenExpiresAt":{later}}}}}"#
    );
    assert_eq!(
        claude_token(&valid, now)
            .unwrap()
            .1
            .unwrap()
            .unix_timestamp_nanos()
            / 1_000_000,
        i128::from(later)
    );
}

#[test]
fn claude_login_warning_only_inside_enabled_window() {
    let now = datetime!(2026-11-28 12:00 UTC);
    for (at, days, shown) in [
        (None, 3, false),
        (Some(now), 3, false),
        (Some(now - time::Duration::seconds(1)), 3, false),
        (Some(now + time::Duration::days(3)), 3, true),
        (
            Some(now + time::Duration::days(3) + time::Duration::seconds(1)),
            3,
            false,
        ),
        (Some(now + time::Duration::hours(1)), 3, true),
        (Some(now + time::Duration::hours(1)), 0, false),
    ] {
        let extras = Extras {
            login_expires_at: login_warning(at, now, days),
            ..Extras::default()
        };
        assert_eq!(extras.login_expires_at.is_some(), shown);
        let json = serde_json::to_value(extras).unwrap();
        assert_eq!(json.get("loginExpiresAt").is_some(), shown);
        if shown {
            assert!(json["loginExpiresAt"].as_str().unwrap().ends_with('Z'));
        }
    }
}

#[test]
fn claude_http_401_and_403_offer_relogin_without_changing_codex() {
    let now = datetime!(2026-11-28 12:00 UTC);
    for service in [&CLAUDE, &CODEX] {
        for status in ["HTTP/1.1 401 Unauthorized", "HTTP/1.1 403 Forbidden"] {
            let url = serve_once(status, "");
            let ctx = Context::new(PathBuf::new());
            for lang in [crate::i18n::Lang::Ko, crate::i18n::Lang::En] {
                // HTTP 한 번씩 응답하므로 언어별 서버를 둔다.
                let url = if lang == crate::i18n::Lang::Ko {
                    url.clone()
                } else {
                    serve_once(status, "")
                };
                crate::i18n::with_lang(lang, || {
                    let Err(Fail(reason, kind)) = tauri::async_runtime::block_on(get_json(
                        &ctx,
                        service,
                        &url,
                        HeaderMap::new(),
                        now,
                        Duration::from_secs(2),
                    )) else {
                        panic!()
                    };
                    assert_eq!(kind, ReasonKind::Setup);
                    let expected = if service.name == "Claude" {
                        claude_login_rejected()
                    } else {
                        login_expired(&CODEX)
                    };
                    assert_eq!(reason, expected);
                });
            }
        }
    }
}
