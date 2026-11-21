//! 자료 변화(뉴스 · 공시 · 재무 · 거시)로 깨우기와 지켜보는 종목의 시세 조건 목록(6차).
//!
//! 자료를 가져오고 "새것"을 가리는 것은 pi-data, 잇는 것은 server다. 이 파일은 받은 사건을
//! 중복 막기 · 속도 제한 · 묶어 보내기로 다듬어 깨운다(원본: Hermes webhook의 중복 키 · 속도 제한 ·
//! 묶어 보내기). 원본은 묶음의 마지막 사건 하나만 보내지만, 우리는 묶음의 사건을 모두(상한
//! [`DataTriggerConfig::batch_limit`]) 싣는다 — 의도해서 다름(결정 2026-11-21 13번).

use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Market, Symbol};
use rust_decimal::Decimal;

use crate::engine::lock;
use crate::{ConditionJob, ConditionKind, ConditionMemory, Error, JobId, Target, Trigger};

/// 지켜보는 종목 시세 조건의 기본 변동률: 3(%). 추정 · 미확정 값이고 설정으로 바꾼다(결정 2026-11-21 13번).
pub const DEFAULT_WATCH_CHANGE_PERCENT: Decimal = Decimal::from_parts(3, 0, 0, false, 0);

/// 자료 변화의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DataKind {
    /// 뉴스.
    News,
    /// 공시.
    Disclosure,
    /// 재무.
    Financials,
    /// 거시.
    Macro,
}

/// 자료 변화 사건 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct DataChange {
    /// 종류.
    pub kind: DataKind,
    /// 공급자 ID(예: `naver` · `dart` · `ecos` · `fred`).
    pub source: String,
    /// 출처 안의 번호(뉴스 링크 · 공시 접수번호 · 재무는 `종목/보고 기간` · 거시는 `계열 id/관측 시점`).
    pub id: String,
    /// 값이 바뀐 것을 가르는 지문(재무 · 거시의 값 지문). 뉴스 · 공시는 `None`.
    pub revision: Option<String>,
    /// 시장.
    pub market: Market,
    /// 종목. 거시 · 시장 뉴스는 없다.
    pub symbol: Option<Symbol>,
    /// 가린 짧은 글(200자에서 자른다).
    pub title: String,
    /// 출처가 낸 시각.
    pub published_at: DateTime<Utc>,
    /// 받은 시각(P10).
    pub learned_at: DateTime<Utc>,
}

/// 자료 변화로 깨우기의 설정. [`crate::TriggerConfig::new`]가 기본값을 채운다.
#[derive(Debug, Clone, PartialEq)]
pub struct DataTriggerConfig {
    /// 받는 종류. 없는 종류는 [`DataPush::Disabled`]. 기본은 네 종류 모두.
    pub enabled: Vec<DataKind>,
    /// 깨울 대상. 기본 `Target::Member("scout")`(뉴스 공시 시세 담당).
    pub target: Target,
    /// 같은 키 `(source, id, revision)`를 중복으로 보는 기간. 기본 1시간.
    pub dedupe_for: Duration,
    /// 묶음의 마지막 사건 뒤 이만큼 조용하면 깨운다. 기본 30초.
    pub quiet: Duration,
    /// 묶음의 첫 사건 뒤 이만큼 지나면 계속 와도 깨운다. 기본 300초.
    pub max_wait: Duration,
    /// 출처마다 속도 제한의 창. 기본 60초.
    pub per_source_window: Duration,
    /// 출처마다 창 하나에 받는 수. 기본 30.
    pub per_source_limit: u32,
    /// 한 깨우기(와 장 밖 보관)에 싣는 사건 상한. 넘으면 오래된 것부터 버린다. 기본 20.
    pub batch_limit: usize,
}

/// [`Trigger::push_data`]의 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPush {
    /// 묶음에 넣었다.
    Queued,
    /// `dedupe_for` 안에 같은 키가 왔었다. 넣지 않았다.
    Duplicate,
    /// 받지 않는 종류다. 넣지 않았다.
    Disabled,
    /// 출처의 속도 제한을 넘었다. 넣지 않았고 중복 표에도 넣지 않았다(뒤에 다시 받을 수 있다).
    RateLimited,
}

/// 지켜보는 종목들로 시세 조건 목록을 만든다: 종목마다 `ConditionKind::Change { reference: 기준가, percent }`
/// 하나, id `price:<시장>:<종목>:change`(시장은 `kr` · `us`). 기준가가 0 이하인 종목은 뺀다.
pub fn watch_conditions(
    market: Market,
    watched: &[(Symbol, Decimal)],
    percent: Decimal,
    target: Target,
) -> Vec<ConditionJob> {
    let name = format!("{market:?}").to_lowercase();
    watched
        .iter()
        .filter(|(_, reference)| reference.is_sign_positive() && !reference.is_zero())
        .map(|(symbol, reference)| ConditionJob {
            id: JobId::new(format!("price:{name}:{}:change", symbol.as_str())),
            market,
            symbol: symbol.clone(),
            kind: ConditionKind::Change {
                reference: *reference,
                percent,
            },
            target: target.clone(),
        })
        .collect()
}

impl Trigger {
    /// 자료 변화 사건 하나를 받는다. 기다리지 않고 저장소를 쓰지 않는다.
    ///
    /// 순서: 받지 않는 종류면 `Disabled` → 같은 키가 `dedupe_for` 안에 왔으면 `Duplicate` →
    /// 출처의 속도 제한을 넘으면 `RateLimited` → 묶음 `(market, symbol)`에 넣고 `Queued`.
    pub fn push_data(&self, change: DataChange) -> DataPush {
        let now = self.now();
        lock(&self.data).push(change, now, &self.config.data)
    }

    /// 시세 조건 목록을 실행 중에 바꾼다. [`Trigger::new`]와 같은 확인을 하고, 실패하면 지금 목록 그대로다.
    /// 같은 id · 같은 조건은 다시 걸기 상태를 그대로 두고, 같은 id인데 조건이 바뀌었으면 `Armed`로 다시 건다.
    /// 빠진 조건은 더 보지 않는다.
    pub async fn replace_conditions(&self, conditions: Vec<ConditionJob>) -> Result<(), Error> {
        // tick과 같은 잠금 순서: tick 잠금 → 쓰기 잠금.
        let _running = self.running.lock().await;
        crate::engine::validate(&self.config.schedules, &conditions, &self.ports.markets)?;
        let _writes = self.inflight.writes.lock().await;
        let old = self.conditions();
        let changed: Vec<&ConditionJob> = conditions
            .iter()
            .filter(|job| {
                old.iter().any(|prev| {
                    prev.id == job.id
                        && (prev.market != job.market
                            || prev.symbol != job.symbol
                            || prev.kind != job.kind)
                })
            })
            .collect();
        if !changed.is_empty() {
            for mut state in self.ports.store.jobs().await? {
                if !changed.iter().any(|job| job.id == state.id) || state.condition.is_none() {
                    continue;
                }
                state.condition = Some(ConditionMemory::Armed);
                self.ports.store.save_job(&state).await?;
                tracing::info!(job = %state.id, "조건이 바뀌어 다시 건다");
            }
        }
        let watched: Vec<(Market, Symbol)> = conditions
            .iter()
            .map(|job| (job.market, job.symbol.clone()))
            .collect();
        lock(&self.watches).retain(|key, _| watched.contains(key));
        *lock(&self.conditions) = conditions;
        Ok(())
    }
}
