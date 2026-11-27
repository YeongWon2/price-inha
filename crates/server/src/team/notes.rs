//! (10차 10c 12번) 기억 잇기: 조건 검색의 매매 카탈로그(실수 유형은 저장소의 복기 기록에서), 날짜 조건
//! (`from` · `to`, 그 시장 달력의 날짜), 논의 때 받은 공시 · 뉴스를 바깥 자료(`external`)로 색인하기.
//!
//! 바깥 자료는 명시 검색(도구)에는 나오고 자동 회상에는 나오지 않는다(M7, `pi_knowledge`가 거른다).

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use pi_ai::Redactor;
use pi_data::{Fact, FactKind, FactValue, NewsItem};
use pi_knowledge::{RecordText, TradeCatalog, TradeRecord};
use pi_store::UnixMillis;
use serde_json::Value;

use super::facts::Gathered;
use super::news::NewsGathered;
use super::review::mistake_word;
use super::{Crew, TeamError};

/// 날짜 글자의 모양(그 시장 달력의 날짜).
const DATE_FORMAT: &str = "%Y-%m-%d";
/// 한국 시각의 UTC 차(시간). DART 공시의 접수일은 한국 날짜다.
const KST_HOURS: i64 = 9;

/// 조건 검색에 넘길 매매 요약들과 매매마다의 실수 유형(복기 기록의 기계 글자).
pub(super) struct Records {
    pub(super) trades: Vec<TradeRecord>,
    pub(super) mistakes: HashMap<String, Vec<String>>,
}

impl Records {
    /// 실수를 모르는 카탈로그(자동 회상용).
    pub(super) fn plain(trades: Vec<TradeRecord>) -> Records {
        Records {
            trades,
            mistakes: HashMap::new(),
        }
    }
}

impl TradeCatalog for Records {
    fn trades(&self) -> Vec<TradeRecord> {
        self.trades.clone()
    }

    fn mistakes(&self, record_ref: &str) -> Vec<String> {
        self.mistakes.get(record_ref).cloned().unwrap_or_default()
    }
}

fn refused(why: &str) -> TeamError {
    TeamError::Knowledge(why.to_string())
}

/// 입력의 날짜 칸(`"YYYY-MM-DD"`). 없으면 없음, 읽을 수 없으면 오류.
pub(super) fn date_of_input(input: &Value, name: &str) -> Result<Option<NaiveDate>, TeamError> {
    match &input[name] {
        Value::Null => Ok(None),
        Value::String(text) => NaiveDate::parse_from_str(text.trim(), DATE_FORMAT)
            .map(Some)
            .map_err(|_| refused("날짜는 YYYY-MM-DD 글자다")),
        _ => Err(refused("날짜는 YYYY-MM-DD 글자다")),
    }
}

/// 한국 날짜의 0시(UTC 밀리초).
fn kst_midnight(date: NaiveDate) -> Option<UnixMillis> {
    let start = date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis();
    Some(UnixMillis(start - KST_HOURS * 3_600_000))
}

fn millis(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

/// 글의 짧은 지문(FNV-1a 64비트, 다시 켜도 같다).
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// 공시 하나를 바깥 자료 기록으로(제목 = 보고서명, 요약 = 회사 · 접수일, 링크 = 출처 주소, 시각 = 접수일).
fn disclosure_record(fact: &Fact) -> RecordText {
    let report = match &fact.value {
        FactValue::Text(text) => text.clone(),
        FactValue::Number(number) => number.to_string(),
    };
    // (round64, 10c N3 참고) 접수번호가 없거나 비면 회사 · 안 시각 · 보고서 이름의 지문으로 키를 지어, 같은 때 받은
    // 다른 공시가 같은 기록 키로 서로 덮어쓰지 않게 한다.
    let receipt = fact
        .reference
        .clone()
        .filter(|r| !r.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{}-{}-{:016x}",
                fact.subject,
                fact.known_at.timestamp_millis(),
                fingerprint(&report)
            )
        });
    let filed = fact
        .published_on
        .map_or_else(|| "-".to_string(), |d| d.format(DATE_FORMAT).to_string());
    let summary = format!(
        "{} · 공시 · 접수일 {filed} · 접수번호 {receipt}",
        fact.label
    );
    let published = fact
        .published_on
        .and_then(kst_midnight)
        .unwrap_or_else(|| millis(fact.known_at));
    RecordText::external(
        &format!("disclosure:{receipt}"),
        &report,
        &summary,
        &fact.source.url,
        published,
    )
}

/// 뉴스 기사 하나를 바깥 자료 기록으로.
fn article_record(item: &NewsItem) -> RecordText {
    RecordText::external(
        &format!("news:{}", item.link),
        &item.title,
        &item.summary,
        &item.link,
        millis(item.published_at),
    )
}

impl Crew {
    /// 조건 검색의 카탈로그: 팔린(복기됨 포함) 매매와, 저장소에 남은 복기 기록의 실수 유형.
    pub(super) async fn catalog(&self) -> Result<Records, TeamError> {
        let trades = self.trading.sold_records().await?;
        let reviews = self
            .journal
            .reviews()
            .map_err(|error| TeamError::Journal(error.to_string()))?;
        let mut mistakes: HashMap<String, Vec<String>> = HashMap::new();
        for review in reviews {
            mistakes
                .entry(review.trade.0.to_string())
                .or_default()
                .extend(review.mistakes.iter().map(mistake_word));
        }
        Ok(Records { trades, mistakes })
    }

    /// 닫힌 시각의 시장 날짜가 `[from, to]`(양끝 포함) 안인 매매만 남긴다.
    pub(super) fn within_dates(
        &self,
        records: &mut Records,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) {
        if from.is_none() && to.is_none() {
            return;
        }
        records.trades.retain(|trade| {
            let at = DateTime::from_timestamp_millis(trade.closed_at.0).unwrap_or_default();
            let day = self.trading.date_of(at);
            from.is_none_or(|from| day >= from) && to.is_none_or(|to| day <= to)
        });
    }

    /// 논의 때 받은 공시 · 뉴스를 가린 글로 바깥 자료(`market_note` · `external`)로 색인한다.
    /// 같은 기록(같은 글)은 다시 올려도 바뀌지 않는다.
    pub(super) fn index_external(&self, gathered: &Gathered, news: Option<&NewsGathered>) {
        let Some(links) = self.links.get() else {
            return;
        };
        let disclosures = gathered
            .disclosures
            .iter()
            .filter(|fact| fact.kind == FactKind::Disclosure)
            .map(disclosure_record);
        let articles = news
            .into_iter()
            .flat_map(|news| news.articles.iter())
            .map(article_record);
        for mut record in disclosures.chain(articles) {
            record.text = self.redactor.redact(&record.text);
            if let Err(error) = links.knowledge.index(record) {
                tracing::warn!(%error, "바깥 자료를 색인하지 못했다");
            }
        }
    }
}
