//! (69차) 끝난 논의의 표시 칸(M2 상태표는 그대로 — 표시만 더한다).
//!
//! 논의가 지켜보기로 끝나면 매매는 논의중 그대로다(`pi_agent` 팀 실행기: 방 로그에 까닭 코드만 남긴다).
//! 그래서 매매 목록 · 매매 하나 · 근거 경로는 그 매매의 방(`trade-<번호>`) 로그를 읽어 **마지막 논의 결과**를 칸으로
//! 더한다.
//!
//! - 매매가 논의중일 때만 본다. 판단됨 이후의 매매에는 칸이 없다.
//! - 방 로그를 번호 차례로 읽는다. `room.activity`의 `{"status": "watch", "reason_code"}`(실행기가 남김)는
//!   지켜보기 끝, `{"status": "failed", "reason_code"}`(서버가 남김, [`note_failed`])는 오류 끝이다. 그 뒤 새 논의의
//!   줄 — `message.member` · `turn.failed` · `room.activity`의 `wrap_up` — 이 있으면 논의가 다시 열린 것이라 끝이
//!   없다. 그 밖의 줄(논의 뒤 서버가 남기는 버린 뉴스 수 · 알림 글 등)은 끝을 바꾸지 않는다.
//! - 칸은 `watched: {"reason_code", "at"}` 또는 `failed: {"reason_code", "at"}`(끝 줄의 시각, 유닉스 밀리초)이다.
//!   끝이 없으면 칸 자체가 없다.
//! - 상태 거르기([`EndFilter`]): `discussing`은 끝이 없는 논의중, `watched` · `failed`는 그 끝인 논의중 매매다.

use pi_store::{
    HISTORY_PAGE_MAX, NewRoomEvent, RoomActorKind, RoomEvent, Store, StoreError, TradeId,
    TradeListItem, TradeState, UnixMillis,
};
use serde_json::{Map, Value, json};

/// 서버가 남기는 오류 끝의 까닭 코드: 팀 실행기의 논의가 오류로 끝났다.
pub(crate) const DISCUSSION_ERROR: &str = "discussion_error";
/// 서버가 남기는 오류 끝의 까닭 코드: 논의 전에 자료를 모으다 오류가 났다.
pub(crate) const GATHER_FAILED: &str = "gather_failed";

/// 끝난 논의의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndKind {
    /// 지켜보기로 끝났다.
    Watched,
    /// 오류로 끝났다.
    Failed,
}

/// 끝난 논의 하나: 종류 · 까닭 코드 · 끝 줄의 시각.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscussionEnd {
    kind: EndKind,
    reason_code: String,
    at: i64,
}

/// 방 로그 줄 하나가 논의의 끝에 주는 뜻.
enum Line {
    End(DiscussionEnd),
    Opened,
    Other,
}

fn line_of(event: &RoomEvent) -> Line {
    let status = event.payload["status"].as_str();
    let end = |kind| {
        Line::End(DiscussionEnd {
            kind,
            reason_code: event.payload["reason_code"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            at: event.at.0,
        })
    };
    match (event.kind.as_str(), status) {
        ("message.member" | "turn.failed", _) | ("room.activity", Some("wrap_up")) => Line::Opened,
        ("room.activity", Some("watch")) => end(EndKind::Watched),
        ("room.activity", Some("failed")) => end(EndKind::Failed),
        _ => Line::Other,
    }
}

/// 방 로그 줄(번호 차례)의 마지막 논의 결과.
fn last_end(events: &[RoomEvent]) -> Option<DiscussionEnd> {
    events.iter().fold(None, |end, event| match line_of(event) {
        Line::End(found) => Some(found),
        Line::Opened => None,
        Line::Other => end,
    })
}

fn room_of(trade: TradeId) -> String {
    format!("{}{}", crate::TRADE_ROOM_PREFIX, trade.0)
}

/// 그 매매가 논의중이면 그 방 로그의 마지막 논의 결과. 논의중이 아니거나 끝이 없으면 없음.
pub(crate) fn discussion_end(
    store: &Store,
    trade: TradeId,
    state: TradeState,
) -> Result<Option<DiscussionEnd>, StoreError> {
    if state != TradeState::Discussing {
        return Ok(None);
    }
    let room = room_of(trade);
    let mut events = Vec::new();
    loop {
        let after = events.last().map_or(0, |event: &RoomEvent| event.seq);
        let page = store.room_events(&room, after, HISTORY_PAGE_MAX)?;
        let full = page.len() == HISTORY_PAGE_MAX;
        events.extend(page);
        if !full {
            return Ok(last_end(&events));
        }
    }
}

/// 몸통에 더할 칸: `watched` 또는 `failed` 하나. 끝이 없으면 빈 칸.
pub(crate) fn end_fields(end: Option<&DiscussionEnd>) -> Map<String, Value> {
    let mut fields = Map::new();
    if let Some(end) = end {
        let name = match end.kind {
            EndKind::Watched => "watched",
            EndKind::Failed => "failed",
        };
        fields.insert(
            name.to_string(),
            json!({ "reason_code": end.reason_code, "at": end.at }),
        );
    }
    fields
}

/// 그 매매의 논의중 몸통에 끝난 논의 칸을 더한다(논의중이 아니면 그대로).
pub(crate) fn with_end(
    store: &Store,
    trade: TradeId,
    state: TradeState,
    mut body: Value,
) -> Result<Value, StoreError> {
    let end = discussion_end(store, trade, state)?;
    if let Value::Object(map) = &mut body {
        map.extend(end_fields(end.as_ref()));
    }
    Ok(body)
}

/// 논의중을 셋으로 나눈 상태 거르기.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndFilter {
    /// 끝이 없는 논의중(`discussing`).
    Open,
    /// 지켜보기로 끝난 논의중(`watched`).
    Watched,
    /// 오류로 끝난 논의중(`failed`).
    Failed,
}

impl EndFilter {
    /// 거르기 글자 → 거르기. 논의중을 나누는 글자가 아니면 없음.
    pub(crate) fn parse(word: &str) -> Option<EndFilter> {
        match word {
            "discussing" => Some(EndFilter::Open),
            "watched" => Some(EndFilter::Watched),
            "failed" => Some(EndFilter::Failed),
            _ => None,
        }
    }

    fn keeps(self, end: Option<&DiscussionEnd>) -> bool {
        matches!(
            (self, end.map(|end| end.kind)),
            (EndFilter::Open, None)
                | (EndFilter::Watched, Some(EndKind::Watched))
                | (EndFilter::Failed, Some(EndKind::Failed))
        )
    }
}

/// 걸러 읽은 쪽 하나: 줄과 그 끝, 다음 쪽의 `before` 값.
pub(crate) type EndedPage = (Vec<(TradeListItem, Option<DiscussionEnd>)>, Option<i64>);

/// 논의중 매매를 최근 것부터 읽으며 거르기에 맞는 것을 `max`개까지 모은다. 하나 더 있으면 `next`는 쪽 마지막 번호.
pub(crate) fn ended_page(
    store: &Store,
    before: Option<TradeId>,
    max: usize,
    filter: EndFilter,
) -> Result<EndedPage, StoreError> {
    let mut kept = Vec::new();
    let mut cursor = before;
    'pages: loop {
        let rows = store.trade_page(cursor, HISTORY_PAGE_MAX, Some(TradeState::Discussing))?;
        let full = rows.len() == HISTORY_PAGE_MAX;
        for row in rows {
            cursor = Some(row.id);
            let end = discussion_end(store, row.id, row.state)?;
            if filter.keeps(end.as_ref()) {
                kept.push((row, end));
                if kept.len() > max {
                    break 'pages;
                }
            }
        }
        if !full {
            break;
        }
    }
    let next = if kept.len() > max {
        kept.truncate(max);
        kept.last().map(|(row, _)| row.id.0)
    } else {
        None
    };
    Ok((kept, next))
}

/// 서버가 그 매매의 방에 오류 끝(`room.activity {"status": "failed", "reason_code"}`)을 남긴다.
/// 매매 상태는 바꾸지 않는다. 남기지 못하면 경고만 남긴다.
pub(crate) fn note_failed(store: &Store, trade: TradeId, code: &str, at: UnixMillis) {
    let event = NewRoomEvent {
        room: room_of(trade),
        event_id: format!("server-failed-{}-{}", trade.0, at.0),
        kind: "room.activity".to_string(),
        actor_kind: RoomActorKind::Gateway,
        actor_id: "server".to_string(),
        payload: json!({ "status": "failed", "reason_code": code }),
        at,
    };
    if let Err(error) = store.append_room_event(event) {
        tracing::warn!(%error, trade = trade.0, "논의 오류 끝을 방 로그에 남기지 못했습니다");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: u64, kind: &str, payload: Value) -> RoomEvent {
        RoomEvent {
            room: "trade-1".to_string(),
            seq,
            event_id: format!("e-{seq}"),
            kind: kind.to_string(),
            actor_kind: RoomActorKind::Gateway,
            actor_id: "agent".to_string(),
            payload,
            at: UnixMillis(i64::try_from(seq).unwrap_or_default() * 10),
        }
    }

    #[test]
    fn 정리_모드_줄은_새_논의로_보고_뉴스_줄은_끝을_바꾸지_않는다() {
        let watch = event(
            1,
            "room.activity",
            json!({"status": "watch", "reason_code": "missing_data"}),
        );
        let note = event(2, "room.activity", json!({"news_dropped": 1}));
        let wrap = event(3, "room.activity", json!({"status": "wrap_up"}));
        let end = last_end(&[watch.clone(), note.clone()]).unwrap();
        assert_eq!(
            (end.kind, end.reason_code.as_str(), end.at),
            (EndKind::Watched, "missing_data", 10)
        );
        assert!(last_end(&[watch, note, wrap]).is_none());
    }
}
