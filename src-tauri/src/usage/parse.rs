//! 응답·기록·CLI 출력 파서. I/O 없음. 형식이 바뀌면 0을 내지 않고 `Fail`로 원인을 알린다(0003).

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use time::format_description::well_known::{Rfc2822, Rfc3339};
use time::{Date, OffsetDateTime, Time};

use super::{LimitWindow, ReasonKind, WindowId};
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
pub fn codex_api(body: &Value, now: OffsetDateTime) -> Result<Vec<LimitWindow>, Fail> {
    let limit = object(body, "rate_limit", "rate_limit")?;
    [
        ("primary_window", WindowId::FiveHour),
        ("secondary_window", WindowId::SevenDay),
    ]
    .into_iter()
    .map(|(key, id)| {
        let entry = object(limit, key, &format!("rate_limit.{key}"))?;
        let at = epoch_at(entry.get("reset_at"));
        limit_window(id, number(entry, "used_percent"), at, now, key)
    })
    .collect()
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
    if !limits.get("primary")?.is_object() || !limits.get("secondary")?.is_object() {
        return None;
    }
    let timestamp = iso_at(row.get("timestamp")).ok().flatten()?;
    Some((timestamp, limits))
}

pub fn codex_rollout_windows(
    limits: &Value,
    now: OffsetDateTime,
) -> Result<Vec<LimitWindow>, Fail> {
    [
        ("primary", WindowId::FiveHour),
        ("secondary", WindowId::SevenDay),
    ]
    .into_iter()
    .map(|(key, id)| {
        let entry = object(limits, key, &format!("rollout {key}"))?;
        let at = epoch_at(entry.get("resets_at"));
        limit_window(
            id,
            number(entry, "used_percent"),
            at,
            now,
            &format!("rollout {key}"),
        )
    })
    .collect()
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
        let no_secondary = json!({"rate_limit": {"primary_window": {"used_percent": 1}}});
        assert_eq!(
            codex_api(&no_secondary, NOW),
            fail("rate_limit.secondary_window 없음")
        );
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
        let unreadable = fail("primary_window: 리셋 시각을 읽을 수 없음");
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
