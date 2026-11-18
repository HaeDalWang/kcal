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
            // 사용량 API는 리다이렉트할 일이 없다. 따라가면 계정 헤더가 다른 호스트로 갈 수 있다.
            .redirect(reqwest::redirect::Policy::none())
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
        "Claude 구독 로그인이 없어요. API 키·Bedrock·Vertex를 쓰면 보여 줄 한도가 없고, 시작 컨텍스트와 점검은 그대로 쓸 수 있어요. Pro·Max라면 claude를 실행해 로그인한 뒤 새로고침하세요.",
        "No Claude subscription login found. With an API key, Bedrock or Vertex there is no limit to show; Startup context and Checks still work. On Pro or Max, run claude and sign in, then press Refresh.",
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

#[path = "sources_codex_log.rs"]
mod codex_log;
use codex_log::codex_from_log;

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
