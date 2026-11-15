//! 에이전트별 조회 I/O: 토큰 읽기(읽기 전용, refresh 없음), HTTP, 대체 기록, kiro-cli.
//!
//! 토큰은 `Secret`에만 담는다. `Secret`은 Debug에서 가려지고 Display·Serialize가 없다.
//! 실패한 응답의 본문, 프로세스 출력, 하위 오류 메시지는 reason에 넣지 않는다.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, SystemTime};

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use serde_json::Value;
use time::OffsetDateTime;

use super::parse::{self, Fail, fail, setup};
use super::{Agent, AgentUsage, LimitWindow, ReasonKind, UsageSource, paths};
use crate::i18n::pick;
use crate::tr;
use crate::tuning::Tuning;

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CODEX_RATE_LIMITS_MARK: &str = "\"rate_limits\":{";

/// 토큰·계정 id. 값은 헤더를 만들 때만 꺼낸다.
pub struct Secret(String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Clone)]
pub struct Context {
    pub home: PathBuf,
    /// TLS 초기화에 실패하면 `None`. 그때 API 경로는 reason을 남기고 대체 경로로 간다.
    client: Option<reqwest::Client>,
    claude_url: String,
    codex_url: String,
}

impl Context {
    pub fn new(home: PathBuf) -> Self {
        Self::with_urls(home, CLAUDE_USAGE_URL.into(), CODEX_USAGE_URL.into())
    }

    fn with_urls(home: PathBuf, claude_url: String, codex_url: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("kcal/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok();
        Self {
            home,
            client,
            claude_url,
            codex_url,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimit {
    pub retry_after_secs: Option<u64>,
}

#[derive(Debug)]
pub struct Outcome {
    pub usage: AgentUsage,
    /// API가 429였으면 `Some`. 스케줄러가 간격을 늘린다.
    pub rate_limit: Option<RateLimit>,
}

struct Reading {
    observed_at: Option<OffsetDateTime>,
    windows: Vec<LimitWindow>,
}

enum Api {
    Value(Reading),
    RateLimited(Option<u64>),
}

impl Outcome {
    fn new(
        agent: Agent,
        value: Option<(UsageSource, Reading)>,
        reason: Option<Fail>,
        rate_limit: Option<RateLimit>,
    ) -> Self {
        let (reason, reason_kind) = match reason {
            Some(Fail(text, kind)) => (Some(text), Some(kind)),
            None => (None, None),
        };
        let (source, observed_at, windows) = match value {
            Some((source, r)) => (Some(source), r.observed_at, r.windows),
            None => (None, None, Vec::new()),
        };
        Self {
            usage: AgentUsage {
                agent,
                source,
                observed_at,
                windows,
                reason,
                reason_kind,
                checked_at: Some(OffsetDateTime::now_utc()),
            },
            rate_limit,
        }
    }

    pub fn failed(agent: Agent, reason: Fail) -> Self {
        Self::new(agent, None, Some(reason), None)
    }
}

/// `tuning`: 시간 초과(HTTP·Keychain·kiro-cli)와 대체 기록을 읽는 범위. 매 조회마다 현재 값을 받는다.
pub async fn fetch(agent: Agent, ctx: &Context, tuning: &Tuning) -> Outcome {
    let now = OffsetDateTime::now_utc();
    match agent {
        Agent::Claude => claude(ctx, now, tuning).await,
        Agent::Codex => codex(ctx, now, tuning).await,
        Agent::Kiro => kiro(ctx, now, tuning).await,
    }
}

/// API를 먼저 쓰고, 실패하면 API를 못 쓴 이유와 함께 대체 기록 값을 쓴다.
/// 대체 기록이 아예 없으면(`Ok(None)`: 파일·rollout 없음) API 이유만 보인다. 새 사용자에게 없는 기록 이야기는 소음이다.
async fn with_log_fallback<F>(agent: Agent, api: Result<Api, Fail>, read_log: F) -> Outcome
where
    F: FnOnce() -> Result<Option<Reading>, Fail> + Send + 'static,
{
    let (Fail(reason, kind), rate_limit) = match api {
        Ok(Api::Value(reading)) => {
            return Outcome::new(agent, Some((UsageSource::Api, reading)), None, None);
        }
        Ok(Api::RateLimited(retry_after_secs)) => (
            Fail(
                pick(
                    "요청이 몰려 잠시 막혔어요. kcal이 다음 확인 때 다시 시도해요.",
                    "Too many requests right now. kcal retries on the next check.",
                )
                .to_owned(),
                ReasonKind::Failure,
            ),
            Some(RateLimit { retry_after_secs }),
        ),
        Err(api_fail) => (api_fail, None),
    };
    let log = tauri::async_runtime::spawn_blocking(read_log)
        .await
        .unwrap_or_else(|_| {
            fail(pick(
                "대체 기록 읽기 중 내부 오류",
                "internal error while reading the fallback log",
            ))
        });
    match log {
        Ok(Some(reading)) => Outcome::new(
            agent,
            Some((UsageSource::Log, reading)),
            Some(Fail(reason, kind)),
            rate_limit,
        ),
        Ok(None) => Outcome::new(agent, None, Some(Fail(reason, kind)), rate_limit),
        // 종류는 API 쪽 이유를 따른다. 대체 기록의 문제는 덧붙는 설명일 뿐이다.
        Err(Fail(log_reason, _)) => {
            eprintln!("fallback log read failed: {log_reason}");
            Outcome::new(agent, None, Some(Fail(reason, kind)), rate_limit)
        }
    }
}

enum Http {
    Body(Value),
    RateLimited(Option<u64>),
}

fn bearer(token: &Secret) -> Result<HeaderValue, Fail> {
    let mut value = HeaderValue::from_str(&format!("Bearer {}", token.0))
        .or_else(|_| fail(pick("토큰 형식이 잘못됨", "malformed token")))?;
    value.set_sensitive(true);
    Ok(value)
}

/// 사용량 API를 가진 서비스. 실패 문구에 쓰는 이름과 로그인 명령.
struct Service {
    name: &'static str,
    login: &'static str,
    cli: &'static str,
}

const CLAUDE: Service = Service {
    name: "Claude",
    login: "Claude Code",
    cli: "claude",
};
const CODEX: Service = Service {
    name: "Codex",
    login: "Codex",
    cli: "codex",
};

fn login_expired(service: &Service) -> String {
    tr!(
        "{} 로그인이 만료됐어요. {}를 한 번 실행한 뒤 새로고침하세요.",
        "Your {} login expired. Run {} once, then press Refresh.",
        service.login,
        service.cli
    )
}

fn cannot_reach(service: &Service) -> String {
    tr!(
        "{}에 연결하지 못했어요. 인터넷 연결을 확인하세요. kcal이 다음 확인 때 다시 시도해요.",
        "Can't reach {}. Check your connection; kcal retries on the next check.",
        service.name
    )
}

fn service_trouble(service: &Service) -> String {
    tr!(
        "{}에 지금 문제가 있어요. kcal이 다음 확인 때 다시 시도해요.",
        "{} is having trouble right now. kcal retries on the next check.",
        service.name
    )
}

fn unexpected_answer(service: &Service) -> String {
    tr!(
        "{} 응답이 예상과 달라요. kcal이 다음 확인 때 다시 시도해요.",
        "{} sent an unexpected answer. kcal retries on the next check.",
        service.name
    )
}

/// 실패 응답의 본문은 읽지 않는다. 하위 오류 문자열도 쓰지 않는다. HTTP 상태 번호는 화면에 내지 않는다(상세 칸이 없다).
async fn get_json(
    ctx: &Context,
    service: &Service,
    url: &str,
    headers: HeaderMap,
    now: OffsetDateTime,
    timeout: Duration,
) -> Result<Http, Fail> {
    let Some(client) = &ctx.client else {
        return fail(pick(
            "HTTP 클라이언트 초기화 실패",
            "couldn't start the HTTP client",
        ));
    };
    let response = match client
        .get(url)
        .headers(headers)
        .timeout(timeout)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return fail(cannot_reach(service)),
    };
    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let header = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok());
        return Ok(Http::RateLimited(parse::retry_after_seconds(header, now)));
    }
    if !status.is_success() {
        return match status.as_u16() {
            // 다시 로그인하면 풀린다: 사용자가 할 일.
            401 | 403 => setup(login_expired(service)),
            500..=599 => fail(service_trouble(service)),
            _ => fail(unexpected_answer(service)),
        };
    }
    let bytes = response
        .bytes()
        .await
        .or_else(|_| fail(unexpected_answer(service)))?;
    serde_json::from_slice(&bytes)
        .map(Http::Body)
        .or_else(|_| fail(unexpected_answer(service)))
}

fn api_reading(
    http: Http,
    now: OffsetDateTime,
    parse: fn(&Value, OffsetDateTime) -> Result<Vec<LimitWindow>, Fail>,
) -> Result<Api, Fail> {
    match http {
        Http::RateLimited(seconds) => Ok(Api::RateLimited(seconds)),
        Http::Body(body) => Ok(Api::Value(Reading {
            observed_at: Some(now),
            windows: parse(&body, now)?,
        })),
    }
}

// ---- Claude ----

/// Claude Code credentials JSON(Keychain 값 또는 파일)에서 access token만 꺼낸다.
fn claude_token(credentials: &str, now: OffsetDateTime) -> Result<Secret, Fail> {
    let json: Value = serde_json::from_str(credentials).or_else(|_| {
        fail(pick(
            "Claude 인증 정보가 JSON이 아님",
            "Claude credentials aren't JSON",
        ))
    })?;
    let oauth = json.get("claudeAiOauth");
    let Some(token) = oauth
        .and_then(|o| o.get("accessToken"))
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
    else {
        return fail(pick(
            "Claude 인증 정보에 access token 없음",
            "no access token in Claude credentials",
        ));
    };
    let expires_ms = oauth
        .and_then(|o| o.get("expiresAt"))
        .and_then(Value::as_f64);
    let now_ms = (now.unix_timestamp_nanos() / 1_000_000) as f64;
    if expires_ms.is_some_and(|at| at <= now_ms) {
        // kcal은 설계상 토큰을 갱신하지 않는다. Claude Code를 한 번 쓰면 Claude Code가 갱신한다.
        return setup(login_expired(&CLAUDE));
    }
    Ok(Secret(token.to_owned()))
}

/// `security`가 "항목 없음"(errSecItemNotFound)일 때 내는 종료 코드. 이 Mac에서 직접 확인했다(findings 원장).
#[cfg(target_os = "macos")]
const SECURITY_ITEM_NOT_FOUND: i32 = 44;

#[cfg(target_os = "macos")]
async fn claude_credentials(_home: &Path, keychain_timeout: Duration) -> Result<Secret, Fail> {
    read_keychain(
        Path::new("/usr/bin/security"),
        paths::CLAUDE_KEYCHAIN_SERVICE,
        keychain_timeout,
    )
    .await
}

#[cfg(target_os = "macos")]
async fn read_keychain(
    program: &Path,
    service: &str,
    keychain_timeout: Duration,
) -> Result<Secret, Fail> {
    let args = ["find-generic-password", "-s", service, "-w"];
    match run(program, &args, keychain_timeout, RunInput::default()).await {
        Ok(out) if out.status.success() => {
            String::from_utf8(out.stdout).map(Secret).or_else(|_| {
                fail(pick(
                    "Keychain 항목이 UTF-8이 아님",
                    "Keychain item isn't UTF-8",
                ))
            })
        }
        // macOS가 허용 여부를 묻는 창을 띄워 둔 채 시간이 지난 경우.
        Err(RunError::Timeout) => setup(pick(
            "macOS가 kcal의 Claude Code 로그인 읽기를 허용할지 묻고 있을 수 있어요. 허용을 누른 뒤 새로고침하세요.",
            "macOS may be asking whether kcal can read your Claude Code login. Click Allow, then press Refresh.",
        )),
        // 항목 없음(로그인 전). 로그인하면 풀린다.
        Ok(out) if out.status.code() == Some(SECURITY_ITEM_NOT_FOUND) => setup(claude_sign_in()),
        // 그 밖의 종료 코드는 거절인지 확인하지 못했다. 허용 안내와 로그인 안내를 함께 준다.
        Ok(_) | Err(RunError::Spawn) => setup(pick(
            "Claude Code 로그인을 읽지 못했어요. macOS가 묻는다면 허용을 누르고, 아니면 claude를 실행해 로그인한 뒤 새로고침하세요. API 키·Bedrock·Vertex를 쓰면 보여 줄 한도가 없어요.",
            "Can't read your Claude Code login. If macOS asks, click Allow; otherwise run claude and sign in, then press Refresh. If you use an API key, Bedrock or Vertex, there is no limit to show.",
        )),
    }
}

fn claude_sign_in() -> &'static str {
    pick(
        "Claude Code 로그인을 읽지 못했어요. claude를 실행해 로그인한 뒤 새로고침하세요. API 키·Bedrock·Vertex를 쓰면 보여 줄 한도가 없어요.",
        "Can't read your Claude Code login. Run claude and sign in, then press Refresh. If you use an API key, Bedrock or Vertex, there is no limit to show.",
    )
}

#[cfg(not(target_os = "macos"))]
async fn claude_credentials(home: &Path, _keychain_timeout: Duration) -> Result<Secret, Fail> {
    std::fs::read_to_string(paths::claude_credentials_file(home))
        .map(Secret)
        .or_else(|_| setup(claude_sign_in()))
}

/// 에이전트가 이 컴퓨터에 있는가: 설정 폴더(Claude는 `~/.claude.json`도), Kiro는 kiro-cli도 본다.
/// 첫 실행의 기본 표시 목록과 "설치 안 됨" 문구가 같은 규칙을 쓴다.
pub fn is_installed(agent: Agent, home: &Path, path_env: Option<&std::ffi::OsStr>) -> bool {
    use crate::context::paths as roots;
    match agent {
        Agent::Claude => {
            roots::claude_root(home).is_dir() || roots::claude_state_file(home).is_file()
        }
        Agent::Codex => roots::codex_root(home).is_dir(),
        Agent::Kiro => {
            roots::kiro_root(home).is_dir()
                || paths::kiro_cli_candidates(home, path_env)
                    .iter()
                    .any(|p| is_executable(p))
        }
    }
}

async fn claude(ctx: &Context, now: OffsetDateTime, tuning: &Tuning) -> Outcome {
    let api = async {
        let credentials = claude_credentials(&ctx.home, tuning.keychain_timeout())
            .await
            .or_else(|e| {
                if is_installed(Agent::Claude, &ctx.home, None) {
                    Err(e)
                } else {
                    setup(pick(
                        "kcal이 Claude Code를 찾지 못했어요(~/.claude 폴더나 ~/.claude.json 없음). claude를 한 번 실행해 로그인한 뒤 새로고침하세요. 아직 설치 전이면: https://code.claude.com/docs/en/setup",
                        "kcal didn't find Claude Code (no ~/.claude folder or ~/.claude.json). Run claude once and sign in, then press Refresh. Not installed yet? https://code.claude.com/docs/en/setup",
                    ))
                }
            })?;
        let token = claude_token(&credentials.0, now)?;
        drop(credentials);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, bearer(&token)?);
        headers.insert(
            HeaderName::from_static("anthropic-beta"),
            HeaderValue::from_static("oauth-2025-04-20"),
        );
        api_reading(
            get_json(
                ctx,
                &CLAUDE,
                &ctx.claude_url,
                headers,
                now,
                tuning.http_timeout(),
            )
            .await?,
            now,
            parse::claude_api,
        )
    }
    .await;
    let home = ctx.home.clone();
    let tail_bytes = tuning.claude_log_tail_bytes;
    with_log_fallback(Agent::Claude, api, move || {
        claude_from_log(&home, now, tail_bytes)
    })
    .await
}

/// statusLine 기록의 마지막 rate limit 줄. 기록 파일이 없으면(대부분의 사용자) `Ok(None)`.
fn claude_from_log(
    home: &Path,
    now: OffsetDateTime,
    tail_bytes: u64,
) -> Result<Option<Reading>, Fail> {
    let tail = match read_tail(&paths::claude_statusline_log(home), tail_bytes) {
        Ok(tail) => tail,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return fail(pick("statusLine 기록 없음", "no statusLine log")),
    };
    let latest = tail
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|row| parse::claude_log_line(&row, now));
    match latest {
        Some(Ok((observed_at, windows))) => Ok(Some(Reading {
            observed_at,
            windows,
        })),
        Some(Err(e)) => Err(e),
        None => fail(pick(
            "statusLine 기록에 rate limit 줄 없음",
            "no rate limit line in the statusLine log",
        )),
    }
}

fn read_tail(file: &Path, bytes: u64) -> std::io::Result<String> {
    let mut handle = File::open(file)?;
    let size = handle.metadata()?.len();
    handle.seek(SeekFrom::Start(size.saturating_sub(bytes)))?;
    let mut buffer = Vec::new();
    handle.read_to_end(&mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

// ---- Codex ----

/// `~/.codex/auth.json`의 access token과 account id.
fn codex_tokens(auth: &str) -> Result<(Secret, Secret), Fail> {
    let json: Value = serde_json::from_str(auth)
        .or_else(|_| fail(pick("auth.json이 JSON이 아님", "auth.json isn't JSON")))?;
    let field = |key: &str| {
        json.pointer(&format!("/tokens/{key}"))
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map(|v| Secret(v.to_owned()))
    };
    let Some(token) = field("access_token") else {
        return fail(pick(
            "auth.json에 access token 없음",
            "no access token in auth.json",
        ));
    };
    let Some(account) = field("account_id") else {
        return fail(pick(
            "auth.json에 account id 없음",
            "no account id in auth.json",
        ));
    };
    Ok((token, account))
}

async fn codex(ctx: &Context, now: OffsetDateTime, tuning: &Tuning) -> Outcome {
    let api =
        async {
            let auth = std::fs::read_to_string(paths::codex_auth_file(&ctx.home)).or_else(|_| {
            if is_installed(Agent::Codex, &ctx.home, None) {
                setup(pick(
                    "Codex 로그인을 읽지 못했어요. codex를 실행해 로그인한 뒤 새로고침하세요.",
                    "Can't read your Codex login. Run codex and sign in, then press Refresh.",
                ))
            } else {
                setup(pick("Codex가 설치돼 있지 않아요.", "Codex isn't installed."))
            }
        })?;
            let (token, account) = codex_tokens(&auth)?;
            drop(auth);
            let mut account_header = HeaderValue::from_str(&account.0)
                .or_else(|_| fail(pick("account id 형식이 잘못됨", "malformed account id")))?;
            account_header.set_sensitive(true);
            let mut headers = HeaderMap::new();
            headers.insert(AUTHORIZATION, bearer(&token)?);
            headers.insert(
                HeaderName::from_static("chatgpt-account-id"),
                account_header,
            );
            api_reading(
                get_json(
                    ctx,
                    &CODEX,
                    &ctx.codex_url,
                    headers,
                    now,
                    tuning.http_timeout(),
                )
                .await?,
                now,
                parse::codex_api,
            )
        }
        .await;
    let home = ctx.home.clone();
    let rollouts_checked = tuning.codex_rollouts_checked;
    with_log_fallback(Agent::Codex, api, move || {
        codex_from_log(&home, now, rollouts_checked)
    })
    .await
}

/// 가장 최근에 쓰인 rollout 몇 개에서 가장 새 `rate_limits`. rollout이 하나도 없으면 `Ok(None)`.
// ponytail: `.jsonl.zst` 압축 rollout은 읽지 않는다(0004 열린 것). 필요하면 zstd crate.
fn codex_from_log(
    home: &Path,
    now: OffsetDateTime,
    rollouts_checked: usize,
) -> Result<Option<Reading>, Fail> {
    let mut rollouts = Vec::new();
    walk_rollouts(&paths::codex_sessions_dir(home), &mut rollouts);
    if rollouts.is_empty() {
        return Ok(None);
    }
    rollouts.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    let latest = rollouts
        .iter()
        .take(rollouts_checked)
        .filter_map(|(_, file)| latest_rate_limits(file))
        .max_by_key(|(at, _)| *at);
    let Some((observed_at, limits)) = latest else {
        return fail(pick(
            "rate_limits가 있는 rollout 없음",
            "no rollout with rate_limits",
        ));
    };
    Ok(Some(Reading {
        observed_at: Some(observed_at),
        windows: parse::codex_rollout_windows(&limits, now)?,
    }))
}

fn walk_rollouts(dir: &Path, found: &mut Vec<(SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            walk_rollouts(&path, found);
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("rollout-") && name.ends_with(".jsonl") {
            found.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), path));
        }
    }
}

/// 줄 단위로 읽는다(세션 로그는 수백 MB가 될 수 있다). 표시 문자열이 없는 줄은 JSON 파싱하지 않는다.
fn latest_rate_limits(file: &Path) -> Option<(OffsetDateTime, Value)> {
    let mut reader = BufReader::new(File::open(file).ok()?);
    let mut line = Vec::new();
    let mut latest: Option<(OffsetDateTime, Value)> = None;
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let Ok(text) = std::str::from_utf8(&line) else {
            continue;
        };
        if !text.contains(CODEX_RATE_LIMITS_MARK) {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if let Some((at, limits)) = parse::codex_rollout_line(&row)
            && latest.as_ref().is_none_or(|(prev, _)| at > *prev)
        {
            latest = Some((at, limits.clone()));
        }
    }
    latest
}

// ---- Kiro ----

pub(crate) fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// kiro-cli는 Windows에서 21초까지 걸렸다(docs/platform/windows-log.md). 기본 시간 초과는 그 위로 잡았다.
async fn kiro(ctx: &Context, now: OffsetDateTime, tuning: &Tuning) -> Outcome {
    let timeout = tuning.kiro_timeout();
    let path_env = std::env::var_os("PATH");
    let Some(cli) = paths::kiro_cli_candidates(&ctx.home, path_env.as_deref())
        .into_iter()
        .find(|p| is_executable(p))
    else {
        return Outcome::failed(
            Agent::Kiro,
            Fail(
                pick(
                    "Kiro CLI가 설치돼 있지 않아요.",
                    "Kiro CLI isn't installed.",
                )
                .to_owned(),
                ReasonKind::Setup,
            ),
        );
    };
    let args = ["chat", "--v2", "--no-interactive", "/usage"];
    let out = match run(&cli, &args, timeout, RunInput::default()).await {
        Ok(out) => out,
        Err(RunError::Timeout) => {
            return Outcome::failed(
                Agent::Kiro,
                Fail(
                    pick(
                        "Kiro가 제시간에 답하지 않았어요. kcal이 다음 확인 때 다시 시도해요.",
                        "Kiro didn't answer in time. kcal retries on the next check.",
                    )
                    .to_owned(),
                    ReasonKind::Failure,
                ),
            );
        }
        Err(RunError::Spawn) => {
            return Outcome::failed(
                Agent::Kiro,
                Fail(
                    pick(
                        "Kiro CLI를 실행하지 못했어요. 설치를 확인한 뒤 새로고침하세요.",
                        "Can't run Kiro CLI. Check that it's installed, then press Refresh.",
                    )
                    .to_owned(),
                    ReasonKind::Failure,
                ),
            );
        }
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed = if out.status.success() {
        parse::kiro_cli(&text, now)
    } else {
        fail(pick(
            "Kiro CLI가 사용량을 알려 주지 못했어요. kiro-cli를 실행해 로그인했는지 확인한 뒤 새로고침하세요.",
            "Kiro CLI couldn't report your usage. Run kiro-cli and make sure you're signed in, then press Refresh.",
        ))
    };
    match parsed {
        Ok(windows) => Outcome::new(
            Agent::Kiro,
            Some((
                UsageSource::Cli,
                Reading {
                    observed_at: Some(now),
                    windows,
                },
            )),
            None,
            None,
        ),
        // kiro-cli는 로그인이 안 되면 stderr에 "Not logged in"을 내고 exit 1로 끝난다(확인됨, 빈 HOME).
        // 출력이 실제로 그렇게 말할 때만 로그인 안내를 한다.
        Err(_) if text.to_ascii_lowercase().contains("not logged in") => Outcome::failed(
            Agent::Kiro,
            Fail(
                pick(
                    "Kiro에 로그인돼 있지 않아요. kiro-cli login을 실행해 로그인한 뒤 새로고침하세요.",
                    "You're not signed in to Kiro. Run kiro-cli login, then press Refresh.",
                )
                .to_owned(),
                ReasonKind::Setup,
            ),
        ),
        Err(reason) => Outcome::failed(Agent::Kiro, reason),
    }
}

pub(crate) enum RunError {
    Spawn,
    Timeout,
}

/// 실행 입력. `stdin`이 있으면 그 파일을 표준 입력으로 준다(내용이 프로세스 목록에 보이지 않게).
/// `cwd`가 있으면 그 폴더에서 실행한다. 둘 다 없으면 지금까지의 `/usage`·Keychain 호출과 같다.
#[derive(Default)]
pub(crate) struct RunInput<'a> {
    pub stdin: Option<File>,
    pub cwd: Option<&'a Path>,
}

/// 타임아웃이 지나면 자식 프로세스를 죽인다(`kill_on_drop`). Windows에서는 콘솔 창을 띄우지 않는다.
pub(crate) async fn run(
    program: &Path,
    args: &[&str],
    timeout: Duration,
    input: RunInput<'_>,
) -> Result<Output, RunError> {
    let mut command = tokio::process::Command::new(program);
    let stdin = input.stdin.map_or_else(Stdio::null, Stdio::from);
    command
        .args(args)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = input.cwd {
        command.current_dir(cwd);
    }
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(_)) => Err(RunError::Spawn),
        Err(_) => Err(RunError::Timeout),
    }
}

#[cfg(test)]
mod tests {
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
        let auth =
            format!(r#"{{"tokens": {{"access_token": "{TOKEN}", "account_id": "{ACCOUNT}"}}}}"#);
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
        let outcome = tauri::async_runtime::block_on(codex(
            &ctx,
            OffsetDateTime::now_utc(),
            &Tuning::default(),
        ));
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
            "Can't read your Claude Code login. Run claude and sign in, then press Refresh. If you use an API key, Bedrock or Vertex, there is no limit to show."
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
        let outcome = tauri::async_runtime::block_on(codex(
            &ctx,
            OffsetDateTime::now_utc(),
            &Tuning::default(),
        ));
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
        let valid =
            format!(r#"{{"claudeAiOauth": {{"accessToken": "{TOKEN}", "expiresAt": 9e15}}}}"#);
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
        let outcome = tauri::async_runtime::block_on(codex(
            &ctx,
            OffsetDateTime::now_utc(),
            &Tuning::default(),
        ));
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
            tauri::async_runtime::block_on(codex(
                &ctx,
                OffsetDateTime::now_utc(),
                &Tuning::default(),
            ))
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
            tauri::async_runtime::block_on(codex(
                &ctx,
                OffsetDateTime::now_utc(),
                &Tuning::default(),
            ))
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
            tauri::async_runtime::block_on(kiro(
                &ctx,
                OffsetDateTime::now_utc(),
                &Tuning::default(),
            ))
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
        let outcome =
            tauri::async_runtime::block_on(kiro(&ctx, OffsetDateTime::now_utc(), &tuning));
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
}
