//! (10차 10b 보강 1번) 바깥 출처의 하루 요청 상한: 한국 날짜로 그날 쓴 수를 세어 저장소에 남긴다(다시 켜도
//! 그날 수를 잇는다). 네이버 뉴스([`super::news::NewsDesk`])와 DART([`DartQuota`])가 같은 방식으로 센다.
//!
//! [`DartQuota`]는 자료 어댑터를 감싸 DART 요청(공시 · 재무)마다 하루 수를 하나 쓴다. 주기 가져오기 · 장 시작의
//! 공시 보기 · 논의의 자료 받기가 모두 이 어댑터를 지나므로 함께 센다. 상한에 닿으면 바깥에 묻지 않고
//! `DataError::RateLimited`(DART)로 답한다 — 부르는 쪽은 이것을 "없음(한도)"로 두고 자료 이상으로 세지 않는다.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use pi_data::{DataAdapter, DataError, FactKind, FactRequest, Fetched, SourceName};
use pi_market::Clock;
use pi_store::Store;

/// 그날 DART 요청 수를 남기는 저장소 칸.
pub(super) const DART_USED_KEY: &str = "data_poll.dart_used";

/// (16차 3번) 그날 팀 논의로 AI를 부른 횟수를 남기는 저장소 칸.
pub(super) const AI_USED_KEY: &str = "team.ai_used";

/// 한국 날짜(하루 상한은 한국 날짜로 센다).
pub(super) fn kst_date(at: DateTime<Utc>) -> NaiveDate {
    (at + chrono::Duration::hours(9)).date_naive()
}

/// 하루 요청 상한 하나(저장소 칸 하나).
pub(super) struct DailyQuota {
    store: Store,
    key: &'static str,
    limit: u32,
    /// (날짜, 그날 쓴 수). 처음 쓸 때 저장소에서 읽는다.
    used: Mutex<Option<(NaiveDate, u32)>>,
}

impl DailyQuota {
    pub(super) fn new(store: Store, key: &'static str, limit: u32) -> DailyQuota {
        DailyQuota {
            store,
            key,
            limit,
            used: Mutex::new(None),
        }
    }

    /// 오늘(한국 날짜) 요청 하나를 쓴다. 상한에 닿았으면 거짓. 센 수는 저장소에 남긴다.
    pub(super) fn take(&self, now: DateTime<Utc>) -> bool {
        self.take_within(now, self.limit)
    }

    /// (22차 6번) [`DailyQuota::take`]와 같되 상한을 부르는 쪽이 준다(설정 화면이 바꾼 값을 곧바로 쓰려고).
    pub(super) fn take_within(&self, now: DateTime<Utc>, limit: u32) -> bool {
        let today = kst_date(now);
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        let count = match *used {
            Some((date, count)) if date == today => count,
            Some(_) => 0,
            None => self.saved_count(today),
        };
        if count >= limit {
            *used = Some((today, count));
            return false;
        }
        let count = count + 1;
        *used = Some((today, count));
        let text = format!("{} {count}", today.format("%Y-%m-%d"));
        if let Err(error) = self.store.set_knowledge_meta(self.key, &text) {
            tracing::warn!(%error, "하루 요청 수를 남기지 못했습니다");
        }
        true
    }

    /// (51차) 오늘(한국 날짜) 쓴 수를 읽기만 한다(쓰지 않는다).
    pub(super) fn peek(&self, now: DateTime<Utc>) -> u32 {
        let today = kst_date(now);
        let used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        match *used {
            Some((date, count)) if date == today => count,
            Some(_) => 0,
            None => self.saved_count(today),
        }
    }

    /// 저장소에 남긴 그날의 수(다른 날이거나 없으면 0).
    fn saved_count(&self, today: NaiveDate) -> u32 {
        let saved = self.store.knowledge_meta(self.key).ok().flatten();
        let Some(saved) = saved else {
            return 0;
        };
        let mut parts = saved.split(' ');
        let date = parts
            .next()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        let count = parts.next().and_then(|c| c.parse::<u32>().ok());
        match (date, count) {
            (Some(date), Some(count)) if date == today => count,
            _ => 0,
        }
    }
}

/// DART 요청(공시 · 재무)인가.
fn is_dart(request: &FactRequest) -> bool {
    matches!(request.kind(), FactKind::Disclosure | FactKind::Financials)
}

/// DART 하루 상한을 지키는 자료 어댑터(다른 출처는 그대로 넘긴다).
pub(super) struct DartQuota {
    inner: Arc<dyn DataAdapter>,
    quota: DailyQuota,
    clock: Arc<dyn Clock>,
    /// 상한에 닿았다고 마지막으로 남긴 날(같은 날 한 번만 남긴다).
    noted: Mutex<Option<NaiveDate>>,
}

impl DartQuota {
    pub(super) fn new(
        inner: Arc<dyn DataAdapter>,
        quota: DailyQuota,
        clock: Arc<dyn Clock>,
    ) -> DartQuota {
        DartQuota {
            inner,
            quota,
            clock,
            noted: Mutex::new(None),
        }
    }

    /// 그날 처음 상한에 닿았으면 한 번 남긴다.
    fn note_limit(&self, now: DateTime<Utc>) {
        let today = kst_date(now);
        let mut noted = self.noted.lock().unwrap_or_else(PoisonError::into_inner);
        if *noted != Some(today) {
            *noted = Some(today);
            tracing::warn!("오늘 DART 요청 상한에 닿았습니다(공시 없음(한도), 내일 다시 묻습니다)");
        }
    }
}

#[async_trait]
impl DataAdapter for DartQuota {
    async fn fetch(&self, request: &FactRequest) -> Result<Fetched, DataError> {
        if is_dart(request) {
            let now = self.clock.now();
            if !self.quota.take(now) {
                self.note_limit(now);
                return Err(DataError::RateLimited {
                    origin: SourceName::Dart,
                });
            }
        }
        self.inner.fetch(request).await
    }
}

/// 상한 때문에 묻지 않은 DART 요청인가(자료 이상으로 세지 않는다).
pub(super) fn over_limit(error: &DataError) -> bool {
    matches!(
        error,
        DataError::RateLimited {
            origin: SourceName::Dart
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap()
    }

    #[test]
    fn 하루_상한을_넘지_않고_날이_바뀌면_다시_센다() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("quota.db")).unwrap();
        let quota = DailyQuota::new(store, DART_USED_KEY, 2);
        assert!(quota.take(at(16, 1)));
        assert!(quota.take(at(16, 2)));
        assert!(!quota.take(at(16, 3)));
        assert!(quota.take(at(17, 1)));
    }

    #[test]
    fn 센_수는_다시_켜도_그날_이어진다() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("quota.db")).unwrap();
        let first = DailyQuota::new(store.clone(), DART_USED_KEY, 2);
        assert!(first.take(at(16, 1)));
        assert!(first.take(at(16, 1)));
        let again = DailyQuota::new(store, DART_USED_KEY, 2);
        assert!(!again.take(at(16, 2)));
    }
}
