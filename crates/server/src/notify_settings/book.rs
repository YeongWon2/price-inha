//! 알림 전달처 설정의 값 · 검사 · 파일(원자적 저장) · 지금 값(모듈 문서 [`super`]의 계약).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use pi_market::Market;
use pi_notify::{NoticeKind, QuietHours};
use pi_store::UnixMillis;
use serde_json::{Map, Value, json};

use super::{
    NOTIFY_INVALID_QUIET, NOTIFY_KIND_LABELS, NOTIFY_LOCKED_KINDS, NOTIFY_LOCKED_OFF,
    NOTIFY_QUIET_DEFAULT_END, NOTIFY_QUIET_DEFAULT_START, NOTIFY_SETTINGS_FILE,
};

/// 종류 수(`NoticeKind::ALL`).
const KIND_COUNT: usize = NoticeKind::ALL.len();

/// 한 종류의 전달처.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Route {
    /// 폰 푸시로 보낸다.
    pub(crate) push: bool,
    /// 메신저로 보낸다.
    pub(crate) messenger: bool,
}

/// 조용한 시간(시각은 자정부터 분, 시장 시간대).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Quiet {
    enabled: bool,
    start: u16,
    end: u16,
}

/// 알림 전달처 설정 하나(종류는 `NoticeKind::ALL`의 차례).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotifySettings {
    routes: [Route; KIND_COUNT],
    quiet: Quiet,
}

/// 바꾸기를 받지 않은 까닭(400).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// 몸통 모양이 틀렸다(`invalid_input`).
    Invalid,
    /// 잠긴 종류의 두 전달처를 모두 끄려 했다.
    LockedOff(&'static str),
    /// 조용한 시간의 시각 글자가 틀렸다.
    InvalidQuiet,
}

impl Refusal {
    /// 400 몸통.
    pub(crate) fn body(&self) -> Value {
        match self {
            Refusal::Invalid => json!({ "error": "invalid_input" }),
            Refusal::LockedOff(kind) => json!({ "error": NOTIFY_LOCKED_OFF, "kind": kind }),
            Refusal::InvalidQuiet => json!({ "error": NOTIFY_INVALID_QUIET }),
        }
    }
}

/// 알림 하나를 어디로 보낼지(전달처 고르기의 답).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Plan {
    /// 폰 푸시로 보낸다(조용한 시간에 미루는 것은 notify가 한다).
    pub(crate) push: bool,
    /// 메신저로 보낸다(조용한 시간에 급하지 않은 알림이면 거짓).
    pub(crate) messenger: bool,
    /// 지금의 조용한 시간(notify에 넘긴다). 꺼져 있으면 없음.
    pub(crate) quiet: Option<QuietHours>,
}

/// 기본 시각(분). 상수 글자가 틀릴 일은 없지만 틀리면 0으로.
fn default_minute(text: &str) -> u16 {
    clock_minute(text).unwrap_or(0)
}

impl Default for NotifySettings {
    /// 지금과 같다: 모든 종류가 두 전달처로 · 조용한 시간은 꺼짐.
    fn default() -> Self {
        NotifySettings {
            routes: [Route {
                push: true,
                messenger: true,
            }; KIND_COUNT],
            quiet: Quiet {
                enabled: false,
                start: default_minute(NOTIFY_QUIET_DEFAULT_START),
                end: default_minute(NOTIFY_QUIET_DEFAULT_END),
            },
        }
    }
}

/// `HH:MM`(두 자리 시 00~23 · 두 자리 분 00~59) → 자정부터 분.
fn clock_minute(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return None;
    }
    let digits = |pair: &[u8]| -> Option<u16> {
        pair.iter().try_fold(0u16, |acc, byte| {
            byte.is_ascii_digit()
                .then(|| acc * 10 + u16::from(byte - b'0'))
        })
    };
    let hour = digits(&bytes[..2])?;
    let minute = digits(&bytes[3..])?;
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}

/// 자정부터 분 → `HH:MM`.
fn clock_text(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// 종류의 차례(`NoticeKind::ALL`).
fn index_of(kind: NoticeKind) -> usize {
    NoticeKind::ALL
        .iter()
        .position(|known| *known == kind)
        .unwrap_or(0)
}

/// `kinds` 배열을 읽는다: 여섯 종류가 한 번씩, 값은 참/거짓.
fn parse_kinds(value: Option<&Value>) -> Result<[Route; KIND_COUNT], Refusal> {
    let rows = value.and_then(Value::as_array).ok_or(Refusal::Invalid)?;
    if rows.len() != KIND_COUNT {
        return Err(Refusal::Invalid);
    }
    let mut routes: [Option<Route>; KIND_COUNT] = [None; KIND_COUNT];
    for row in rows {
        let kind = row
            .get("kind")
            .and_then(Value::as_str)
            .and_then(NoticeKind::from_name)
            .ok_or(Refusal::Invalid)?;
        let flag = |name: &str| {
            row.get(name)
                .and_then(Value::as_bool)
                .ok_or(Refusal::Invalid)
        };
        let route = Route {
            push: flag("push")?,
            messenger: flag("messenger")?,
        };
        let slot = &mut routes[index_of(kind)];
        if slot.replace(route).is_some() {
            return Err(Refusal::Invalid);
        }
    }
    let mut out = [Route {
        push: true,
        messenger: true,
    }; KIND_COUNT];
    for (place, route) in out.iter_mut().zip(routes) {
        *place = route.ok_or(Refusal::Invalid)?;
    }
    Ok(out)
}

/// 잠긴 종류를 두 전달처 모두 끄려 했는가.
fn check_locked(routes: &[Route; KIND_COUNT]) -> Result<(), Refusal> {
    for (kind, route) in NoticeKind::ALL.iter().zip(routes) {
        let name = kind.name();
        let locked = NOTIFY_LOCKED_KINDS.iter().find(|locked| **locked == name);
        if let Some(locked) = locked
            && !route.push
            && !route.messenger
        {
            return Err(Refusal::LockedOff(locked));
        }
    }
    Ok(())
}

/// `quiet`를 읽는다. 켜기는 참/거짓(아니면 `invalid_input`), 시각은 `HH:MM`(아니면 `invalid_quiet_hours`).
fn parse_quiet(value: Option<&Value>) -> Result<Quiet, Refusal> {
    let quiet = value.and_then(Value::as_object).ok_or(Refusal::Invalid)?;
    let enabled = quiet
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or(Refusal::Invalid)?;
    let minute = |name: &str| {
        quiet
            .get(name)
            .and_then(Value::as_str)
            .and_then(clock_minute)
            .ok_or(Refusal::InvalidQuiet)
    };
    let (start, end) = (minute("start")?, minute("end")?);
    // 켰는데 시작과 끝이 같으면 notify가 받지 않는다(빈 시간대인지 하루 종일인지 모호).
    if enabled && start == end {
        return Err(Refusal::InvalidQuiet);
    }
    Ok(Quiet {
        enabled,
        start,
        end,
    })
}

impl NotifySettings {
    /// 바꾸기 몸통(파일도 같은 모양)을 읽고 검사한다. `label` · `locked` · `editable`은 무시한다.
    pub(crate) fn parse(body: &Value) -> Result<NotifySettings, Refusal> {
        let object = body.as_object().ok_or(Refusal::Invalid)?;
        let routes = parse_kinds(object.get("kinds"))?;
        if !object.get("quiet").is_some_and(Value::is_object) {
            return Err(Refusal::Invalid);
        }
        check_locked(&routes)?;
        let quiet = parse_quiet(object.get("quiet"))?;
        Ok(NotifySettings { routes, quiet })
    }

    /// 그 종류의 전달처.
    pub(crate) fn route(&self, kind: NoticeKind) -> Route {
        self.routes[index_of(kind)]
    }

    /// 조용한 시간(켜져 있을 때만). 시각은 `offset_minutes`(시장 시간대의 UTC 차이)로 센다.
    fn quiet_hours(&self, offset_minutes: i32) -> Option<QuietHours> {
        if !self.quiet.enabled {
            return None;
        }
        QuietHours::new(self.quiet.start, self.quiet.end, offset_minutes).ok()
    }

    /// 파일에 남기는 모양(바꾸기 몸통과 같다).
    fn file_json(&self) -> Value {
        let kinds: Vec<Value> = NoticeKind::ALL
            .iter()
            .zip(&self.routes)
            .map(|(kind, route)| {
                json!({ "kind": kind.name(), "push": route.push, "messenger": route.messenger })
            })
            .collect();
        json!({ "kinds": kinds, "quiet": self.quiet_json() })
    }

    fn quiet_json(&self) -> Value {
        json!({
            "enabled": self.quiet.enabled,
            "start": clock_text(self.quiet.start),
            "end": clock_text(self.quiet.end),
        })
    }

    /// 읽기 답(`GET`) 모양.
    pub(crate) fn view(&self, editable: bool) -> Value {
        let kinds: Vec<Value> = NoticeKind::ALL
            .iter()
            .zip(&self.routes)
            .map(|(kind, route)| {
                let name = kind.name();
                let label = NOTIFY_KIND_LABELS
                    .iter()
                    .find(|(known, _)| *known == name)
                    .map_or(name, |(_, label)| *label);
                let mut row = Map::new();
                row.insert("kind".into(), json!(name));
                row.insert("label".into(), json!(label));
                row.insert("push".into(), json!(route.push));
                row.insert("messenger".into(), json!(route.messenger));
                row.insert("locked".into(), json!(NOTIFY_LOCKED_KINDS.contains(&name)));
                Value::Object(row)
            })
            .collect();
        json!({ "kinds": kinds, "quiet": self.quiet_json(), "editable": editable })
    }
}

/// 미 동부가 그 시각에 서머타임인가: 3월 둘째 일요일 02:00(표준시, UTC 07:00)부터 11월 첫째 일요일
/// 02:00(서머타임, UTC 06:00)까지.
fn us_eastern_dst(at: DateTime<Utc>) -> bool {
    let year = at.year();
    let sunday = |month: u32, nth: u32| -> Option<NaiveDate> {
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let until = (7 - first.weekday().num_days_from_sunday()) % 7;
        NaiveDate::from_ymd_opt(year, month, 1 + until + 7 * (nth - 1))
    };
    let start = sunday(3, 2).and_then(|d| d.and_hms_opt(7, 0, 0));
    let end = sunday(11, 1).and_then(|d| d.and_hms_opt(6, 0, 0));
    match (start, end) {
        (Some(start), Some(end)) => {
            let at = at.naive_utc();
            start <= at && at < end
        }
        _ => false,
    }
}

/// 그 시각의 시장 시간대와 UTC의 차이(분): 국내는 한국 시각(+9), 미국은 미 동부(서머타임이면 -4, 아니면 -5).
pub(crate) fn market_offset_minutes(market: Market, at: UnixMillis) -> i32 {
    if market == Market::Us {
        let dst = DateTime::<Utc>::from_timestamp_millis(at.0).is_some_and(us_eastern_dst);
        return if dst { -4 * 60 } else { -5 * 60 };
    }
    // 국내(그 밖의 시장도 국내 시각으로 센다 — 알림 시각 글은 한국 시각이다).
    9 * 60
}

/// 설정 파일을 읽는다. 없거나 읽지 못하거나 깨졌으면 기본(모두 보냄) — 알림이 조용히 사라지지 않게.
fn load(home: &Path) -> NotifySettings {
    let path = home.join(NOTIFY_SETTINGS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return NotifySettings::default();
        }
        Err(error) => {
            tracing::warn!(%error, "알림 설정 파일을 읽지 못해 기본(모두 보냄)으로 둡니다");
            return NotifySettings::default();
        }
    };
    let parsed = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| NotifySettings::parse(&value).ok());
    parsed.unwrap_or_else(|| {
        tracing::warn!("알림 설정 파일이 깨져 기본(모두 보냄)으로 둡니다");
        NotifySettings::default()
    })
}

/// 설정 파일에 남긴다(임시 파일에 쓰고 옮긴다).
fn save(home: &Path, settings: &NotifySettings) -> std::io::Result<()> {
    let temp = home.join(format!("{NOTIFY_SETTINGS_FILE}.tmp"));
    let body = settings.file_json().to_string();
    let written = std::fs::write(&temp, body)
        .and_then(|()| std::fs::rename(&temp, home.join(NOTIFY_SETTINGS_FILE)));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// 지금의 알림 전달처 설정(보관 폴더의 파일과 같다). 바꾼 값은 바로 쓰인다.
pub(crate) struct NotifyBook {
    home: PathBuf,
    market: Market,
    current: Mutex<NotifySettings>,
}

impl NotifyBook {
    /// 보관 폴더의 파일을 읽어 연다(읽지 못하면 기본).
    pub(crate) fn open(home: &Path, market: Market) -> NotifyBook {
        NotifyBook {
            home: home.to_path_buf(),
            market,
            current: Mutex::new(load(home)),
        }
    }

    /// 지금 값.
    pub(crate) fn current(&self) -> NotifySettings {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 그 시각의 조용한 시간(켜져 있을 때만, 시장 시간대).
    pub(crate) fn quiet_at(&self, now: UnixMillis) -> Option<QuietHours> {
        self.current()
            .quiet_hours(market_offset_minutes(self.market, now))
    }

    /// 알림 하나를 어디로 보낼지: 종류별 전달처, 조용한 시간이면 급하지 않은 알림은 메신저로도 보내지 않는다.
    pub(crate) fn plan(&self, kind: NoticeKind, now: UnixMillis) -> Plan {
        let settings = self.current();
        let route = settings.route(kind);
        let quiet = settings.quiet_hours(market_offset_minutes(self.market, now));
        let hushed = !kind.ignores_quiet_hours() && quiet.is_some_and(|q| q.contains(now));
        Plan {
            push: route.push,
            messenger: route.messenger && !hushed,
            quiet,
        }
    }

    /// 통째로 바꾼다: 파일에 먼저 남기고(실패하면 값은 그대로) 지금 값을 바꾼다.
    pub(crate) fn replace(&self, next: NotifySettings) -> std::io::Result<()> {
        let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        save(&self.home, &next)?;
        *current = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 시각_글자는_두_자리_시와_분만_받는다() {
        assert_eq!(clock_minute("00:00"), Some(0));
        assert_eq!(clock_minute("23:59"), Some(23 * 60 + 59));
        for bad in [
            "24:00", "12:60", "7:00", "07:0", "0700", "", "7시", "ab:cd", "+1:00",
        ] {
            assert_eq!(clock_minute(bad), None, "{bad}");
        }
        assert_eq!(clock_text(7 * 60 + 5), "07:05");
    }

    #[test]
    fn 미_동부_서머타임은_3월_둘째_일요일부터_11월_첫째_일요일까지다() {
        let at = |text: &str| {
            DateTime::parse_from_rfc3339(text)
                .map(|t| UnixMillis(t.timestamp_millis()))
                .unwrap_or(UnixMillis(0))
        };
        // 2026년: 3월 8일 · 11월 1일이 일요일.
        assert_eq!(
            market_offset_minutes(Market::Us, at("2026-03-08T06:59:00Z")),
            -300
        );
        assert_eq!(
            market_offset_minutes(Market::Us, at("2026-03-08T07:00:00Z")),
            -240
        );
        assert_eq!(
            market_offset_minutes(Market::Us, at("2026-11-01T05:59:00Z")),
            -240
        );
        assert_eq!(
            market_offset_minutes(Market::Us, at("2026-11-01T06:00:00Z")),
            -300
        );
        assert_eq!(
            market_offset_minutes(Market::Kr, at("2026-07-01T00:00:00Z")),
            540
        );
    }

    #[test]
    fn 기본은_모두_보내고_조용한_시간은_꺼짐이다() {
        let settings = NotifySettings::default();
        for kind in NoticeKind::ALL {
            assert_eq!(
                settings.route(kind),
                Route {
                    push: true,
                    messenger: true
                }
            );
        }
        assert_eq!(settings.quiet_hours(540), None);
        let back = NotifySettings::parse(&settings.file_json());
        assert_eq!(back, Ok(settings), "파일 모양을 다시 읽으면 같다");
    }

    #[test]
    fn 조용한_시간을_끈_채로는_시작과_끝이_같아도_받는다() {
        let mut body = NotifySettings::default().file_json();
        body["quiet"] = json!({"enabled": false, "start": "10:00", "end": "10:00"});
        assert!(NotifySettings::parse(&body).is_ok());
        body["quiet"]["enabled"] = json!(true);
        assert_eq!(NotifySettings::parse(&body), Err(Refusal::InvalidQuiet));
    }
}
