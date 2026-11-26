//! (61차) 보관 기간의 요약 한 행: 건수 · 종류별 · 담당별 수 · 언급한 매매 번호 · 고정 틀 한 단락(AI를 부르지 않는다).

use std::collections::{BTreeMap, BTreeSet};

use pi_store::ArchiveCategory;
use serde_json::{Value, json};

/// 요약에 싣는 매매 번호의 가장 많은 수.
const TRADES_MAX: usize = 50;

/// 묶음의 사람이 읽는 이름.
fn label(category: ArchiveCategory) -> &'static str {
    match category {
        ArchiveCategory::Room => "팀 방 대화",
        ArchiveCategory::Screen => "화면 이벤트",
        ArchiveCategory::AiUsage => "AI 사용 세부",
    }
}

/// 내용에서 매매 번호(양수)를 찾는다: `trade` · `tradeId` · `trade_id`.
fn trade_of(payload: &Value) -> Option<i64> {
    ["trade", "tradeId", "trade_id"]
        .iter()
        .find_map(|key| payload.get(key).and_then(Value::as_i64))
        .filter(|id| *id > 0)
}

/// 줄의 담당: 방 로그는 쓴 쪽 id(멤버만), 이벤트는 내용의 `agent`.
fn agent_of(category: ArchiveCategory, line: &Value) -> Option<String> {
    if category == ArchiveCategory::Room {
        return (line["actor_kind"] == "member")
            .then(|| line["actor_id"].as_str().map(str::to_owned))
            .flatten();
    }
    line["payload"]["agent"].as_str().map(str::to_owned)
}

fn counts_text(map: &BTreeMap<String, u64>) -> String {
    if map.is_empty() {
        return "없음".to_owned();
    }
    let parts: Vec<String> = map.iter().map(|(k, n)| format!("{k} {n}")).collect();
    parts.join(" · ")
}

/// 요약(JSON 글). `file`은 보관 폴더 안의 이름.
pub(super) fn build(
    category: ArchiveCategory,
    period: &str,
    lines: &[String],
    file: &str,
) -> String {
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_agent: BTreeMap<String, u64> = BTreeMap::new();
    let mut trades: BTreeSet<i64> = BTreeSet::new();
    for text in lines {
        let Ok(line) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if let Some(kind) = line["kind"].as_str() {
            *by_kind.entry(kind.to_owned()).or_default() += 1;
        }
        if let Some(agent) = agent_of(category, &line) {
            *by_agent.entry(agent).or_default() += 1;
        }
        trades.extend(trade_of(&line["payload"]));
    }
    let trades: Vec<i64> = trades.into_iter().take(TRADES_MAX).collect();
    let mentioned = if trades.is_empty() {
        "없음".to_owned()
    } else {
        let ids: Vec<String> = trades.iter().map(|id| format!("#{id}")).collect();
        ids.join(", ")
    };
    let text = format!(
        "{period} {} {}건을 보관 파일로 옮겼다. 종류별: {}. 담당별: {}. 언급한 매매: {mentioned}. 원문: {}/{file}",
        label(category),
        lines.len(),
        counts_text(&by_kind),
        counts_text(&by_agent),
        super::ARCHIVE_DIR,
    );
    json!({
        "category": category.code(),
        "period": period,
        "rows": lines.len(),
        "by_kind": by_kind,
        "by_agent": by_agent,
        "trades": trades,
        "text": text,
    })
    .to_string()
}

/// 바깥 자료 요약 글(지식 색인에 올린다): 기간 · 건수 · 제목(각 기록의 첫 줄) 몇 개.
pub(super) fn knowledge_text(period: &str, titles: &[String], file: &str) -> String {
    const SHOWN: usize = 20;
    let shown: Vec<&str> = titles.iter().take(SHOWN).map(String::as_str).collect();
    let more = titles.len().saturating_sub(SHOWN);
    let tail = if more > 0 {
        format!(" 외 {more}건")
    } else {
        String::new()
    };
    format!(
        "보관 요약 {period}: 바깥 자료 {}건. 제목: {}{tail}. 원문: {}/{file}",
        titles.len(),
        shown.join(" · "),
        super::ARCHIVE_DIR,
    )
}
