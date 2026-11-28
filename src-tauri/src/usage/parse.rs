//! 응답·기록·CLI 출력 파서. I/O 없음. 형식이 바뀌면 0을 내지 않고 `Fail`로 원인을 알린다(0003).

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use time::format_description::well_known::{Rfc2822, Rfc3339};
use time::{Date, OffsetDateTime, Time};

use super::{Credits, Extras, LimitWindow, ReasonKind, WindowId};
use crate::i18n::pick;
use crate::tr;

/// 사용자에게 보여도 되는 이유와 그 종류. 고정 문구와 필드 이름·HTTP 상태 같은 비밀 아닌 값으로만 만든다.
#[derive(Debug, PartialEq)]
pub struct Fail(pub String, pub ReasonKind);

/// 진짜 실패(네트워크·HTTP·형식·시간 초과).
pub fn fail<T>(message: impl Into<String>) -> Result<T, Fail> {
    Err(Fail(message.into(), ReasonKind::Failure))
}

/// 사용자가 할 일이 남은 상태(설치·로그인·Keychain 허용). 고장이 아니다.
pub fn setup<T>(message: impl Into<String>) -> Result<T, Fail> {
    Err(Fail(message.into(), ReasonKind::Setup))
}

/// 시각 필드. 없음·null은 `Ok(None)`, 읽을 수 없으면 `Err`.
type At = Result<Option<OffsetDateTime>, ()>;

fn iso_at(value: Option<&Value>) -> At {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => OffsetDateTime::parse(s, &Rfc3339).map(Some).map_err(|_| ()),
        Some(_) => Err(()),
    }
}

fn epoch_at(value: Option<&Value>) -> At {
    let seconds = match value {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(_) => None,
    }
    .filter(|s| s.is_finite())
    .ok_or(())?;
    OffsetDateTime::from_unix_timestamp(seconds.floor() as i64)
        .map(Some)
        .map_err(|_| ())
}

fn limit_window(
    id: WindowId,
    used_percent: Option<f64>,
    resets_at: At,
    now: OffsetDateTime,
    field: &str,
) -> Result<LimitWindow, Fail> {
    let Some(used_percent) = used_percent.filter(|p| p.is_finite()) else {
        return fail(tr!("{field}: 사용률 없음", "{field}: no usage percent"));
    };
    let Ok(resets_at) = resets_at else {
        return fail(tr!(
            "{field}: 리셋 시각을 읽을 수 없음",
            "{field}: can't read the reset time"
        ));
    };
    let window = LimitWindow {
        id,
        used_percent,
        resets_at,
        reset_passed: false,
        used: None,
        limit: None,
        group: None,
    };
    Ok(window.at(now))
}

fn object<'a>(value: &'a Value, key: &str, field: &str) -> Result<&'a Value, Fail> {
    match value.get(key) {
        Some(v) if v.is_object() => Ok(v),
        _ => fail(tr!("{field} 없음", "no {field}")),
    }
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

/// 부가 값은 원소 단위로 느슨하게 읽는다. 모양이 틀린 원소는 건너뛰고 기본 창 읽기를 막지 않는다.
fn lenient_number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

fn model_window(
    group: &str,
    percent: Option<f64>,
    resets_at: At,
    now: OffsetDateTime,
) -> Option<LimitWindow> {
    let group = group.trim();
    if group.is_empty() {
        return None;
    }
    let window = limit_window(WindowId::SevenDay, percent, resets_at, now, group).ok()?;
    Some(LimitWindow {
        group: Some(group.to_owned()),
        ..window
    })
}

/// Claude 응답의 모델 전용 주간 창과 추가 사용량. `limits[]`의 `weekly_scoped`가 먼저고,
/// 같은 이름의 `seven_day_opus`·`seven_day_sonnet`은 건너뛴다. `session`·`weekly_all`은 기본 창과 같아 더하지 않는다.
pub fn claude_extras(body: &Value, now: OffsetDateTime) -> Extras {
    let mut model_windows: Vec<LimitWindow> = Vec::new();
    let mut push = |w: Option<LimitWindow>| {
        if let Some(w) = w {
            let same = |o: &LimitWindow| {
                o.group.as_deref().map(str::to_lowercase)
                    == w.group.as_deref().map(str::to_lowercase)
            };
            if !model_windows.iter().any(same) {
                model_windows.push(w);
            }
        }
    };
    for entry in body
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if entry.get("kind").and_then(Value::as_str) != Some("weekly_scoped") {
            continue;
        }
        let Some(name) = entry
            .pointer("/scope/model/display_name")
            .and_then(Value::as_str)
        else {
            continue;
        };
        push(model_window(
            name,
            lenient_number(entry.get("percent")),
            iso_at(entry.get("resets_at")),
            now,
        ));
    }
    for (key, name) in [("seven_day_opus", "Opus"), ("seven_day_sonnet", "Sonnet")] {
        if let Some(entry) = body.get(key).filter(|v| v.is_object()) {
            push(model_window(
                name,
                lenient_number(entry.get("utilization")),
                iso_at(entry.get("resets_at")),
                now,
            ));
        }
    }
    Extras {
        model_windows,
        credits: claude_credits(body),
        reset_passes: None,
        login_expires_at: None,
    }
}

/// `extra_usage`: 켜져 있고 금액 둘이 다 있을 때만. 응답 금액은 최소 단위이고 소수 자릿수는
/// `decimal_places`(없으면 2, 센트 — CodexBar 테스트의 2050 → 20.5와 같다).
fn claude_credits(body: &Value) -> Option<Credits> {
    let extra = body.get("extra_usage")?;
    if extra.get("is_enabled").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let places = extra
        .get("decimal_places")
        .and_then(Value::as_u64)
        .filter(|p| *p <= 6)
        .unwrap_or(2);
    let scale = 10f64.powi(places as i32);
    let used = lenient_number(extra.get("used_credits"))? / scale;
    let limit = lenient_number(extra.get("monthly_limit"))? / scale;
    let currency = extra
        .get("currency")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or("USD");
    Some(Credits {
        used,
        limit,
        currency: currency.to_uppercase(),
    })
}

/// Codex 응답의 모델별 한도(`additional_rate_limits[]`)와 초기화권 개수(`rate_limit_reset_credits.available_count`).
pub fn codex_extras(body: &Value, now: OffsetDateTime) -> Extras {
    let mut model_windows = Vec::new();
    for entry in body
        .get("additional_rate_limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(name), Some(limit)) = (
            entry.get("limit_name").and_then(Value::as_str),
            entry.get("rate_limit").filter(|v| v.is_object()),
        ) else {
            continue;
        };
        for key in ["primary_window", "secondary_window"] {
            let Some(window) = limit.get(key).filter(|v| v.is_object()) else {
                continue;
            };
            let id = if key == "primary_window" {
                WindowId::FiveHour
            } else {
                WindowId::SevenDay
            };
            let at = epoch_at(window.get("reset_at"));
            let used = lenient_number(window.get("used_percent"));
            if let Some(w) = model_window(name, used, at, now) {
                model_windows.push(LimitWindow { id, ..w });
            }
        }
    }
    let reset_passes = lenient_number(body.pointer("/rate_limit_reset_credits/available_count"))
        .filter(|n| *n >= 0.0 && n.fract() == 0.0)
        .map(|n| n as u32);
    Extras {
        model_windows,
        credits: None,
        reset_passes,
        login_expires_at: None,
    }
}

/// `GET api.anthropic.com/api/oauth/usage`: `five_hour`·`seven_day`의 `utilization`, `resets_at`(ISO).
pub fn claude_api(body: &Value, now: OffsetDateTime) -> Result<Vec<LimitWindow>, Fail> {
    [
        ("five_hour", WindowId::FiveHour),
        ("seven_day", WindowId::SevenDay),
    ]
    .into_iter()
    .map(|(key, id)| {
        let entry = object(body, key, key)?;
        let at = iso_at(entry.get("resets_at"));
        limit_window(id, number(entry, "utilization"), at, now, key)
    })
    .collect()
}

/// `GET chatgpt.com/backend-api/wham/usage`: `rate_limit.{primary,secondary}_window`의 `used_percent`, `reset_at`(epoch).
/// 창 하나만 오기도 한다(`secondary_window: null`). 둘 다 없을 때만 실패.
pub fn codex_api(body: &Value, now: OffsetDateTime) -> Result<Vec<LimitWindow>, Fail> {
    let limit = object(body, "rate_limit", "rate_limit")?;
    codex_windows(
        limit,
        [
            ("primary_window", WindowId::FiveHour),
            ("secondary_window", WindowId::SevenDay),
        ],
        "rate_limit",
        |entry| {
            let seconds = number(entry, "limit_window_seconds");
            (seconds, epoch_at(entry.get("reset_at")))
        },
        now,
    )
}

/// 주간으로 보는 창 길이 하한(초). 5시간 창과 7일 창 사이.
const CODEX_WEEKLY_MIN_SECS: f64 = 86_400.0;

/// Codex `primary`·`secondary` 창. 없거나 null인 창은 건너뛴다. 창 길이(초)가 있으면 자리 대신 길이로 5시간·주간을 정한다.
fn codex_windows(
    limits: &Value,
    keys: [(&str, WindowId); 2],
    field: &str,
    read: impl Fn(&Value) -> (Option<f64>, At),
    now: OffsetDateTime,
) -> Result<Vec<LimitWindow>, Fail> {
    let windows = keys
        .into_iter()
        .filter_map(|(key, by_slot)| {
            let entry = limits.get(key).filter(|v| v.is_object())?;
            let (seconds, at) = read(entry);
            let id = match seconds {
                Some(s) if s >= CODEX_WEEKLY_MIN_SECS => WindowId::SevenDay,
                Some(_) => WindowId::FiveHour,
                None => by_slot,
            };
            let name = format!("{field}.{key}");
            Some(limit_window(
                id,
                number(entry, "used_percent"),
                at,
                now,
                &name,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if windows.is_empty() {
        return fail(tr!("{field}: 한도 창 없음", "{field}: no limit window"));
    }
    Ok(windows)
}

/// 기록 시각과 창들.
pub type Logged = (Option<OffsetDateTime>, Vec<LimitWindow>);

/// statusLine 기록 한 줄: `{ts, five_hour: {pct, resets_at}, seven_day: {...}}`(epoch). 해당 없는 줄은 `None`.
pub fn claude_log_line(row: &Value, now: OffsetDateTime) -> Option<Result<Logged, Fail>> {
    let five = row.get("five_hour").filter(|v| v.is_object())?;
    let seven = row.get("seven_day").filter(|v| v.is_object())?;
    let windows: Result<Vec<_>, Fail> = [
        (five, WindowId::FiveHour, "log five_hour"),
        (seven, WindowId::SevenDay, "log seven_day"),
    ]
    .into_iter()
    .map(|(entry, id, field)| {
        let at = epoch_at(entry.get("resets_at"));
        limit_window(id, number(entry, "pct"), at, now, field)
    })
    .collect();
    Some(windows.map(|w| (epoch_at(row.get("ts")).ok().flatten(), w)))
}

/// Codex rollout 한 줄의 `payload.rate_limits`와 `timestamp`. 해당 없는 줄은 `None`.
pub fn codex_rollout_line(row: &Value) -> Option<(OffsetDateTime, &Value)> {
    let limits = row.pointer("/payload/rate_limits")?;
    let has = |key| limits.get(key).is_some_and(Value::is_object);
    if !has("primary") && !has("secondary") {
        return None;
    }
    let timestamp = iso_at(row.get("timestamp")).ok().flatten()?;
    Some((timestamp, limits))
}

pub fn codex_rollout_windows(
    limits: &Value,
    now: OffsetDateTime,
) -> Result<Vec<LimitWindow>, Fail> {
    codex_windows(
        limits,
        [
            ("primary", WindowId::FiveHour),
            ("secondary", WindowId::SevenDay),
        ],
        "rollout",
        |entry| {
            let seconds = number(entry, "window_minutes").map(|m| m * 60.0);
            (seconds, epoch_at(entry.get("resets_at")))
        },
        now,
    )
}

/// CSI·OSC 시퀀스. kiro-cli는 TTY가 아니어도 색을 넣는다.
static ANSI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)").expect("valid regex")
});
static KIRO_CREDITS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Credits \(([\d.,]+) of ([\d.,]+) covered in plan\),\s*([\d.]+)%")
        .expect("valid regex")
});
static KIRO_RESET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"resets on (\d{4})-(\d{2})-(\d{2})").expect("valid regex"));

fn plain_number(text: &str) -> Option<f64> {
    text.replace(',', "").parse().ok()
}

/// /usage 출력에서 숫자를 못 읽었을 때. 로그인 여부는 호출한 쪽이 먼저 가린다.
fn kiro_unreadable() -> &'static str {
    pick(
        "Kiro 사용량을 읽지 못했어요. kcal이 다음 확인 때 다시 시도해요.",
        "Couldn't read Kiro usage; kcal retries on the next check.",
    )
}

/// `kiro-cli chat --v2 --no-interactive /usage` 출력.
pub fn kiro_cli(output: &str, now: OffsetDateTime) -> Result<Vec<LimitWindow>, Fail> {
    let plain = ANSI.replace_all(output, "");
    let Some(credits) = KIRO_CREDITS.captures(&plain) else {
        return fail(kiro_unreadable());
    };
    let Some(reset) = KIRO_RESET.captures(&plain) else {
        return fail(kiro_unreadable());
    };
    // ponytail: kiro는 시간대 없는 날짜만 준다. UTC 자정으로 가정한다. 실제 리셋 시각을 주면 그것을 쓴다.
    let resets_at = (|| {
        let month = time::Month::try_from(reset[2].parse::<u8>().ok()?).ok()?;
        let date = Date::from_calendar_date(reset[1].parse().ok()?, month, reset[3].parse().ok()?);
        Some(date.ok()?.with_time(Time::MIDNIGHT).assume_utc())
    })()
    .ok_or(());
    let window = limit_window(
        WindowId::Month,
        credits[3].parse().ok(),
        resets_at.map(Some),
        now,
        "credits",
    )?;
    Ok(vec![LimitWindow {
        used: plain_number(&credits[1]),
        limit: plain_number(&credits[2]),
        ..window
    }])
}

/// `Retry-After`: 초 또는 HTTP date. 읽을 수 없으면 `None`(간격을 두 배로 늘린다).
pub fn retry_after_seconds(header: Option<&str>, now: OffsetDateTime) -> Option<u64> {
    let value = header?.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value.parse().ok();
    }
    let at = OffsetDateTime::parse(value, &Rfc2822).ok()?;
    let seconds = (at - now).as_seconds_f64().ceil();
    Some(seconds.max(0.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-11-04 12:00 UTC);

    #[test]
    fn parses_claude_api_windows() {
        let body = json!({
            "five_hour": {"utilization": 12.5, "resets_at": "2026-11-04T15:00:00.442155+00:00"},
            "seven_day": {"utilization": 40.0, "resets_at": null},
        });
        let w = claude_api(&body, NOW).unwrap();
        assert_eq!((w[0].id, w[0].used_percent), (WindowId::FiveHour, 12.5));
        assert_eq!(
            w[0].resets_at,
            Some(datetime!(2026-11-04 15:00:00.442155 UTC))
        );
        assert_eq!(
            (w[1].id, w[1].used_percent, w[1].resets_at),
            (WindowId::SevenDay, 40.0, None)
        );
    }

    #[test]
    fn claude_api_fails_with_field_name_when_window_or_percent_missing() {
        let no_seven = json!({"five_hour": {"utilization": 1, "resets_at": null}});
        assert_eq!(claude_api(&no_seven, NOW), fail("seven_day 없음"));
        let no_pct = json!({"five_hour": {"resets_at": null}, "seven_day": {}});
        assert_eq!(claude_api(&no_pct, NOW), fail("five_hour: 사용률 없음"));
        let bad_time =
            json!({"five_hour": {"utilization": 1, "resets_at": "soon"}, "seven_day": {}});
        assert_eq!(
            claude_api(&bad_time, NOW),
            fail("five_hour: 리셋 시각을 읽을 수 없음")
        );
    }

    #[test]
    fn parses_codex_api_windows() {
        let body = json!({"rate_limit": {
            "primary_window": {"used_percent": 7, "reset_at": 1_793_890_800},
            "secondary_window": {"used_percent": 55.5, "reset_at": 1_794_300_000},
        }});
        let w = codex_api(&body, NOW).unwrap();
        assert_eq!(w[0].used_percent, 7.0);
        assert_eq!(w[0].resets_at, Some(datetime!(2026-11-05 15:00 UTC)));
        assert_eq!((w[1].id, w[1].used_percent), (WindowId::SevenDay, 55.5));
    }

    #[test]
    fn codex_api_fails_when_rate_limit_missing() {
        assert_eq!(
            codex_api(&json!({"plan_type": "plus"}), NOW),
            fail("rate_limit 없음")
        );
        let no_window = json!({"rate_limit": {"primary_window": null, "secondary_window": null}});
        assert_eq!(codex_api(&no_window, NOW), fail("rate_limit: 한도 창 없음"));
    }

    #[test]
    fn codex_reads_a_single_window_and_names_it_by_length() {
        let five_only = json!({"rate_limit": {
            "primary_window": {"used_percent": 89, "limit_window_seconds": 18_000, "reset_at": 1_793_890_800},
            "secondary_window": null,
        }});
        let w = codex_api(&five_only, NOW).unwrap();
        assert_eq!((w.len(), w[0].id), (1, WindowId::FiveHour));
        let weekly_in_primary = json!({"rate_limit": {
            "primary_window": {"used_percent": 3, "limit_window_seconds": 604_800, "reset_at": 1_794_300_000},
        }});
        assert_eq!(
            codex_api(&weekly_in_primary, NOW).unwrap()[0].id,
            WindowId::SevenDay
        );
        let rollout = json!({"primary": {"used_percent": 35.0, "window_minutes": 300, "resets_at": 1_794_820_920}, "secondary": null});
        let w = codex_rollout_windows(&rollout, NOW).unwrap();
        assert_eq!((w.len(), w[0].id), (1, WindowId::FiveHour));
        let line =
            json!({"timestamp": "2026-11-05T00:00:00Z", "payload": {"rate_limits": rollout}});
        assert!(codex_rollout_line(&line).is_some());
    }

    #[test]
    fn claude_extras_reads_scoped_weekly_and_skips_duplicates() {
        let body = json!({
            "five_hour": {"utilization": 1, "resets_at": null},
            "seven_day": {"utilization": 2, "resets_at": null},
            "seven_day_opus": {"utilization": 9.5, "resets_at": "2026-11-08T00:00:00Z"},
            "seven_day_sonnet": {"utilization": 4.0, "resets_at": null},
            "limits": [
                {"kind": "session", "group": "session", "percent": 1, "scope": null},
                {"kind": "weekly_all", "group": "weekly", "percent": 2, "scope": null},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 31,
                 "resets_at": "2026-11-08T00:00:00Z",
                 "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}},
                {"kind": "weekly_scoped", "percent": 50,
                 "scope": {"model": {"display_name": "opus"}}},
                {"kind": "weekly_scoped", "percent": "x", "scope": {"model": {"display_name": "Bad"}}},
                {"kind": "weekly_scoped", "percent": 5, "scope": {"model": {"display_name": ""}}},
                "junk",
            ],
        });
        let e = claude_extras(&body, NOW);
        let got: Vec<_> = e
            .model_windows
            .iter()
            .map(|w| (w.group.clone().unwrap(), w.used_percent, w.id))
            .collect();
        assert_eq!(
            got,
            [
                ("Fable".to_owned(), 31.0, WindowId::SevenDay),
                ("opus".to_owned(), 50.0, WindowId::SevenDay),
                ("Sonnet".to_owned(), 4.0, WindowId::SevenDay),
            ]
        );
        assert_eq!(e.credits, None);
        // 기본 창 읽기는 그대로 두 개.
        assert_eq!(claude_api(&body, NOW).unwrap().len(), 2);
    }

    #[test]
    fn claude_extras_ignores_malformed_fields() {
        let body = json!({"limits": "nope", "seven_day_opus": 3, "extra_usage": []});
        assert_eq!(claude_extras(&body, NOW), Extras::default());
    }

    #[test]
    fn claude_credits_need_enabled_and_both_amounts_in_cents() {
        let on = |used: Value, limit: Value| {
            claude_extras(
                &json!({"extra_usage": {"is_enabled": true, "used_credits": used,
                    "monthly_limit": limit, "currency": "usd"}}),
                NOW,
            )
            .credits
        };
        let credits = on(json!(1250), json!("10000")).unwrap();
        assert_eq!(
            (credits.used, credits.limit, credits.currency.as_str()),
            (12.5, 100.0, "USD")
        );
        let yen = json!({"extra_usage": {"is_enabled": true, "used_credits": 500,
            "monthly_limit": 3000, "currency": "JPY", "decimal_places": 0}});
        let credits = claude_extras(&yen, NOW).credits.unwrap();
        assert_eq!((credits.used, credits.limit), (500.0, 3000.0));
        assert_eq!(on(json!(null), json!(100)), None);
        assert_eq!(on(json!(5), json!(null)), None);
        let off =
            json!({"extra_usage": {"is_enabled": false, "used_credits": 1, "monthly_limit": 2}});
        assert_eq!(claude_extras(&off, NOW).credits, None);
        let no_currency =
            json!({"extra_usage": {"is_enabled": true, "used_credits": 100, "monthly_limit": 200}});
        assert_eq!(
            claude_extras(&no_currency, NOW).credits.unwrap().currency,
            "USD"
        );
    }

    #[test]
    fn codex_extras_reads_additional_limits_and_reset_passes() {
        let window = |p: f64| json!({"used_percent": p, "reset_at": 1_794_300_000});
        let body = json!({
            "rate_limit": {"primary_window": window(1.0), "secondary_window": window(2.0)},
            "additional_rate_limits": [
                {"limit_name": "Spark", "metered_feature": "x",
                 "rate_limit": {"primary_window": window(10.0), "secondary_window": window(20.0)}},
                {"limit_name": "NoLimit", "rate_limit": null},
                {"rate_limit": {"primary_window": window(5.0)}},
            ],
            "rate_limit_reset_credits": {"available_count": 3, "applicable_available_count": 1},
        });
        let e = codex_extras(&body, NOW);
        let got: Vec<_> = e
            .model_windows
            .iter()
            .map(|w| (w.group.as_deref().unwrap(), w.id, w.used_percent))
            .collect();
        assert_eq!(
            got,
            [
                ("Spark", WindowId::FiveHour, 10.0),
                ("Spark", WindowId::SevenDay, 20.0)
            ]
        );
        assert_eq!(e.reset_passes, Some(3));
        assert_eq!(codex_api(&body, NOW).unwrap().len(), 2);
        let none = json!({"additional_rate_limits": null,
            "rate_limit_reset_credits": {"available_count": -1}});
        assert_eq!(codex_extras(&none, NOW), Extras::default());
        assert_eq!(codex_extras(&json!({}), NOW), Extras::default());
    }

    #[test]
    fn window_whose_reset_passed_reads_zero_percent() {
        let body = json!({
            "five_hour": {"utilization": 90, "resets_at": "2026-11-04T11:59:59Z"},
            "seven_day": {"utilization": 30, "resets_at": "2026-11-05T00:00:00Z"},
        });
        let w = claude_api(&body, NOW).unwrap();
        assert_eq!((w[0].used_percent, w[0].reset_passed), (0.0, true));
        assert_eq!((w[1].used_percent, w[1].reset_passed), (30.0, false));
    }

    #[test]
    fn parses_claude_statusline_log_line() {
        let row = json!({"ts": 1_793_880_000,
            "five_hour": {"pct": 10, "resets_at": 1_793_890_800},
            "seven_day": {"pct": 20, "resets_at": 1_794_300_000}});
        let (observed, w) = claude_log_line(&row, NOW).unwrap().unwrap();
        assert_eq!(observed, Some(datetime!(2026-11-05 12:00 UTC)));
        assert_eq!((w[0].used_percent, w[1].used_percent), (10.0, 20.0));
        assert!(claude_log_line(&json!({"ts": 1}), NOW).is_none());
    }

    #[test]
    fn parses_codex_rollout_rate_limits() {
        let row = json!({"timestamp": "2026-11-04T11:00:00.000Z", "payload": {"type": "token_count",
            "rate_limits": {"primary": {"used_percent": 3.0, "resets_at": 1_793_890_800},
                            "secondary": {"used_percent": 9.0, "resets_at": 1_794_300_000}}}});
        let (at, limits) = codex_rollout_line(&row).unwrap();
        assert_eq!(at, datetime!(2026-11-04 11:00 UTC));
        let w = codex_rollout_windows(limits, NOW).unwrap();
        assert_eq!((w[0].used_percent, w[1].used_percent), (3.0, 9.0));
        let null_limits =
            json!({"timestamp": "2026-11-04T11:00:00Z", "payload": {"rate_limits": null}});
        assert!(codex_rollout_line(&null_limits).is_none());
    }

    #[test]
    fn parses_kiro_output_with_ansi_colors() {
        let out = "\x1b[1mEstimated Usage\x1b[0m | resets on 2026-12-01 | KIRO PRO MAX\n\
                   \x1b]8;;https://kiro.dev\x07link\x1b]8;;\x07\n\
                   Credits (\x1b[32m1,995.38\x1b[0m of 5,000 covered in plan), \x1b[33m39.9\x1b[0m%\n\
                   Overages: Enabled\n";
        let w = kiro_cli(out, NOW).unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!((w[0].id, w[0].used_percent), (WindowId::Month, 39.9));
        assert_eq!((w[0].used, w[0].limit), (Some(1995.38), Some(5000.0)));
        assert_eq!(w[0].resets_at, Some(datetime!(2026-12-01 0:00 UTC)));
    }

    #[test]
    fn kiro_fails_when_credits_or_reset_line_missing() {
        assert_eq!(
            kiro_cli("Not logged in", NOW),
            fail("Kiro 사용량을 읽지 못했어요. kcal이 다음 확인 때 다시 시도해요.")
        );
        let no_reset = "Credits (1 of 10 covered in plan), 10%";
        assert_eq!(
            kiro_cli(no_reset, NOW),
            fail("Kiro 사용량을 읽지 못했어요. kcal이 다음 확인 때 다시 시도해요.")
        );
    }

    fn codex_reset(reset_at: Value) -> Result<Vec<LimitWindow>, Fail> {
        let window = |reset_at: &Value| json!({"used_percent": 5.0, "reset_at": reset_at});
        let body = json!({"rate_limit": {
            "primary_window": window(&reset_at),
            "secondary_window": window(&json!(1_794_300_000)),
        }});
        codex_api(&body, NOW)
    }

    #[test]
    fn reset_epoch_reads_numeric_strings_and_treats_null_as_unknown() {
        let w = codex_reset(json!("1794300000")).unwrap();
        assert_eq!(w[0].resets_at, Some(datetime!(2026-11-10 8:40 UTC)));
        assert_eq!(codex_reset(json!(null)).unwrap()[0].resets_at, None);
    }

    #[test]
    fn reset_epoch_fails_with_field_name_instead_of_panicking_on_unusable_values() {
        let unreadable = fail("rate_limit.primary_window: 리셋 시각을 읽을 수 없음");
        for bad in [
            json!("soon"),
            json!(""),
            json!("NaN"),
            json!("inf"),
            json!("-inf"),
            json!("1e999"),
            json!(1e30),
            json!(-1e30),
            json!(i64::MAX),
            json!(true),
            json!({}),
            json!([]),
        ] {
            assert_eq!(codex_reset(bad.clone()), unreadable, "input {bad}");
        }
    }

    #[test]
    fn kiro_fails_as_unreadable_reset_when_date_is_impossible() {
        for date in ["2026-13-45", "2026-02-30", "2026-00-10", "2026-12-00"] {
            let out = format!("resets on {date}\nCredits (1 of 10 covered in plan), 10%");
            assert_eq!(
                kiro_cli(&out, NOW),
                fail("credits: 리셋 시각을 읽을 수 없음"),
                "date {date}"
            );
        }
    }

    #[test]
    fn reads_retry_after_as_seconds_or_http_date() {
        assert_eq!(retry_after_seconds(Some(" 120 "), NOW), Some(120));
        assert_eq!(
            retry_after_seconds(Some("Wed, 04 Nov 2026 12:01:30 GMT"), NOW),
            Some(90)
        );
        assert_eq!(
            retry_after_seconds(Some("Wed, 04 Nov 2026 11:00:00 GMT"), NOW),
            Some(0)
        );
        assert_eq!(retry_after_seconds(Some("later"), NOW), None);
        assert_eq!(retry_after_seconds(None, NOW), None);
    }
}
