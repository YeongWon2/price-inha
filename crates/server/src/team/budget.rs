//! (51차, 차수 표 21번) 하루 AI 잔량(C22 · M13 · N14 · E52).
//!
//! - 팀 상한 · 그날 쓴 수는 (54차) AI 호출(턴) 단위다: 담당별 사용과 같은 턴을 같은 때 센다(담당별 사용의 합 =
//!   쓴 수, [`super::turns`]). 상한이 없어도 그날 쓴 수는 센다(잔량 바에 쓴 수를 보인다).
//! - 담당별 사용은 담당마다 그날 쓴 AI 호출 수다: 논의의 담당 턴(논의 · 다시 시도 · 점수 · 팀장 판단)마다 하나,
//!   깨우기 턴 · 공지 확인 답 · 이어 말하기는 그 담당에 하나. 한국 날짜로 새로 세고 저장소에 남긴다(다시 켜도
//!   그날 것은 남는다 — 하루 호출 수를 남기는 방식과 같다).
//! - 팀장 예비분 · 모드 · 담당별 몫(바의 분모)은 N14: 예비분 = 상한의 10%(올림, 최소 2번), 남은 호출이 "예비분 +
//!   상한의 20%(올림)" 이하면 정리 모드(`wrap_up`), 예비분 이하면 팀장 몫만(`leader_only`). 담당 몫 =
//!   (상한 − 예비분) ÷ 6, 팀장은 거기에 예비분을 더한다. 색 단계는 사용률 0.33 · 0.66을 넘을 때 바뀐다.
//! - 읽기 [`TEAM_BUDGET_PATH`] · 바뀔 때 이벤트 [`KIND_TEAM_AI_USED`](같은 모양). 계산과 정렬(많이 쓴 순서)은
//!   여기서 하고 화면은 그린다.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, NaiveDate, Utc};
use pi_agent::Role;
use pi_office::Agent;
use pi_store::Store;
use serde_json::{Value, json};

use super::Crew;
use super::link::office_agent;
use super::quota::kst_date;
use super::turns::{call_budget, mode_name};
use crate::reach::ProgressBoard;

/// 하루 AI 잔량 읽기(C22). `GET` → `{"limit": 수 | null, "used", "reserve", "remaining": 수 | null,
/// "wrap_up_at": 수 | null, "mode": "normal" | "wrap_up" | "leader_only", "ratio": 수 | null,
/// "level": "low" | "mid" | "high" | null, "members": [{"agent", "used", "share": 수 | null, "ratio": 수 | null,
/// "level": … | null}]}`. 담당은 여섯(사무실 id), 많이 쓴 순서(같으면 사무실 순서). 세션이 있어야 열린다.
pub const TEAM_BUDGET_PATH: &str = "/api/team/budget";

/// 하루 AI 잔량이 바뀔 때 내는 이벤트(C22). 내용은 [`TEAM_BUDGET_PATH`]의 답과 같다.
pub const KIND_TEAM_AI_USED: &str = "team.ai_used";

/// 담당별 사용을 남기는 저장소 칸("YYYY-MM-DD news=1 strategy=2 …").
const MEMBERS_USED_KEY: &str = "team.ai_used.members";

/// 사용률 색 단계의 문턱(N14).
const LEVEL_MID: f64 = 0.33;
const LEVEL_HIGH: f64 = 0.66;

/// 담당 수(바의 분모를 나누는 수).
const MEMBER_COUNT: u32 = 6;

/// 담당 여섯의 그날 쓴 호출 수([`Agent::ALL`] 순서).
pub(super) type Tally = [u32; 6];

/// 담당별 그날 사용과 표시판.
pub(crate) struct Meter {
    store: Store,
    /// (날짜, 담당별 수). 처음 쓸 때 저장소에서 읽는다.
    used: Mutex<Option<(NaiveDate, Tally)>>,
    /// 사무실 표시판(3차 조립이 붙인다).
    pub(super) board: OnceLock<Arc<ProgressBoard>>,
    /// 잔량 이벤트를 셈과 같은 차례로 내게 잡는 자물쇠.
    pub(super) order: Mutex<()>,
}

impl Meter {
    pub(super) fn new(store: Store) -> Meter {
        Meter {
            store,
            used: Mutex::new(None),
            board: OnceLock::new(),
            order: Mutex::new(()),
        }
    }

    /// 오늘 그 담당의 호출 하나를 센다(저장소에 남긴다).
    pub(super) fn add(&self, now: DateTime<Utc>, agent: Agent) {
        let today = kst_date(now);
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        let mut tally = self.today(&used, today);
        tally[slot(agent)] += 1;
        *used = Some((today, tally));
        if let Err(error) = self
            .store
            .set_knowledge_meta(MEMBERS_USED_KEY, &tally_text(today, &tally))
        {
            tracing::warn!(%error, "담당별 하루 AI 사용을 남기지 못했습니다");
        }
    }

    /// 오늘 담당별 쓴 수(읽기만).
    fn peek(&self, now: DateTime<Utc>) -> Tally {
        let used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        self.today(&used, kst_date(now))
    }

    fn today(&self, used: &Option<(NaiveDate, Tally)>, today: NaiveDate) -> Tally {
        match *used {
            Some((date, tally)) if date == today => tally,
            Some(_) => [0; 6],
            None => {
                let saved = self.store.knowledge_meta(MEMBERS_USED_KEY).ok().flatten();
                saved
                    .and_then(|text| parse_tally(&text, today))
                    .unwrap_or([0; 6])
            }
        }
    }
}

/// [`Agent::ALL`] 안의 자리(0~5).
fn slot(agent: Agent) -> usize {
    Agent::ALL
        .iter()
        .position(|one| *one == agent)
        .unwrap_or_default()
}

/// 저장 글: "YYYY-MM-DD news=1 strategy=2 …".
fn tally_text(date: NaiveDate, tally: &Tally) -> String {
    let mut text = date.format("%Y-%m-%d").to_string();
    for agent in Agent::ALL {
        text.push_str(&format!(" {}={}", agent.id(), tally[slot(agent)]));
    }
    text
}

/// 저장 글을 읽는다. 다른 날이거나 모양이 틀리면 없음.
fn parse_tally(text: &str, today: NaiveDate) -> Option<Tally> {
    let mut parts = text.split(' ');
    let date = NaiveDate::parse_from_str(parts.next()?, "%Y-%m-%d").ok()?;
    if date != today {
        return None;
    }
    let mut tally = [0; 6];
    for part in parts {
        let (id, count) = part.split_once('=')?;
        let agent = Agent::from_id(id)?;
        tally[slot(agent)] = count.parse().ok()?;
    }
    Some(tally)
}

/// 사용률의 색 단계(0.33 · 0.66을 넘으면 바뀐다).
fn level(ratio: f64) -> &'static str {
    if ratio > LEVEL_HIGH {
        "high"
    } else if ratio > LEVEL_MID {
        "mid"
    } else {
        "low"
    }
}

/// 쓴 수 ÷ 몫. 몫이 0이면 썼으면 1(가득), 안 썼으면 0.
fn ratio(used: u32, share: f64) -> f64 {
    if share > 0.0 {
        f64::from(used) / share
    } else if used > 0 {
        1.0
    } else {
        0.0
    }
}

/// 잔량 내용을 만든다(순수 계산, N14 · M13 · C22).
pub(super) fn budget_json(limit: Option<u32>, used: u32, tally: &Tally) -> Value {
    let mut order: Vec<Agent> = Agent::ALL.to_vec();
    // 많이 쓴 순서(같으면 사무실 순서 — 안정 정렬).
    order.sort_by(|a, b| tally[slot(*b)].cmp(&tally[slot(*a)]));
    let Some(limit) = limit else {
        let members: Vec<Value> = order
            .iter()
            .copied()
            .map(|agent| {
                json!({"agent": agent.id(), "used": tally[slot(agent)], "share": null,
                       "ratio": null, "level": null})
            })
            .collect();
        return json!({
            "limit": null, "used": used, "reserve": 0, "remaining": null, "wrap_up_at": null,
            "mode": "normal", "ratio": null, "level": null, "members": members,
        });
    };
    // (54차) 논의에 넘기는 예산과 같은 값 · 같은 판정을 쓴다.
    let shared = call_budget(limit, used);
    let (reserve, wrap_up_at, remaining) = (shared.reserve, shared.wrap_up_at, shared.remaining);
    let mode = mode_name(shared);
    let base = f64::from(limit.saturating_sub(reserve)) / f64::from(MEMBER_COUNT);
    let members: Vec<Value> = order
        .iter()
        .copied()
        .map(|agent| {
            let share = if agent == Agent::Leader {
                base + f64::from(reserve)
            } else {
                base
            };
            let mine = tally[slot(agent)];
            let r = ratio(mine, share);
            json!({"agent": agent.id(), "used": mine, "share": share, "ratio": r, "level": level(r)})
        })
        .collect();
    let team = ratio(used, f64::from(limit));
    json!({
        "limit": limit, "used": used, "reserve": reserve, "remaining": remaining,
        "wrap_up_at": wrap_up_at, "mode": mode, "ratio": team, "level": level(team),
        "members": members,
    })
}

impl Crew {
    /// (51차) 사무실 표시판을 붙인다(3차 조립).
    pub(crate) fn attach_board(&self, board: Arc<ProgressBoard>) {
        let _ = self.meter.board.set(board);
    }

    /// 지금 잔량(C22).
    pub(crate) fn budget(&self) -> Value {
        let now = self.clock.now();
        budget_json(
            self.day_limit(),
            self.ai_cap.peek(now),
            &self.meter.peek(now),
        )
    }

    /// 잔량이 바뀌었다: 표시판이 있으면 이벤트를 낸다(센 차례대로).
    pub(super) fn publish_budget(&self) {
        let Some(board) = self.meter.board.get() else {
            return;
        };
        let _order = self
            .meter
            .order
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        board.budget(self.budget());
    }

    /// 그 담당의 AI 호출(턴) 하나를 센다. (54차) 팀의 쓴 수에도 하나를 더한다(막지 않는다 — 논의 안의 턴은
    /// agent가 넘겨받은 예산으로 막는다).
    pub(super) fn note_member_call(&self, role: Role) {
        self.count_turn(office_agent(role));
    }

    /// (51차) 공지 확인 답 · 이어 말하기 한 번을 하루 상한에서 잡고, 잡았으면 그 담당의 호출로 센다.
    pub(crate) fn take_spare_ai_call_by(&self, agent: Agent) -> bool {
        let taken = self.take_spare_ai_call();
        if taken {
            self.meter.add(self.clock.now(), agent);
        }
        self.publish_budget();
        taken
    }
}

/// `GET` [`TEAM_BUDGET_PATH`].
pub(super) async fn budget_route(State(crew): State<Arc<Crew>>) -> Response {
    (
        StatusCode::OK,
        [(CACHE_CONTROL, "no-store")],
        axum::Json(crew.budget()),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
    }

    #[test]
    fn 저장_글은_날짜와_담당별_수이고_다른_날이면_읽지_않는다() {
        let tally = [3, 2, 1, 0, 0, 4];
        let text = tally_text(day(), &tally);
        assert_eq!(
            text,
            "2026-11-21 news=3 strategy=2 risk=1 review=0 trader=0 leader=4"
        );
        assert_eq!(parse_tally(&text, day()), Some(tally));
        assert_eq!(parse_tally(&text, day().succ_opt().unwrap()), None);
        assert_eq!(parse_tally("2026-11-21 nobody=1", day()), None);
    }

    #[test]
    fn 상한_10이면_예비분_2_정리_문턱_4이고_남은_수로_모드가_바뀐다() {
        let tally = [0; 6];
        let mode = |used| budget_json(Some(10), used, &tally)["mode"].clone();
        assert_eq!(mode(5), "normal");
        assert_eq!(mode(6), "wrap_up");
        assert_eq!(mode(7), "wrap_up");
        assert_eq!(mode(8), "leader_only");
        assert_eq!(mode(12), "leader_only");
        let at = budget_json(Some(10), 0, &tally);
        assert_eq!(at["reserve"], 2);
        assert_eq!(at["wrap_up_at"], 4);
        let big = budget_json(Some(100), 0, &tally);
        assert_eq!(big["reserve"], 10);
        assert_eq!(big["wrap_up_at"], 30);
    }

    #[test]
    fn 담당_몫은_상한에서_예비분을_뺀_것의_6분의_1이고_팀장은_예비분을_더한다() {
        let tally = [1, 0, 0, 0, 0, 1];
        let budget = budget_json(Some(20), 2, &tally);
        let find = |agent: &str| {
            budget["members"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["agent"] == agent)
                .unwrap()
                .clone()
        };
        assert_eq!(find("news")["share"], 3.0);
        assert_eq!(find("leader")["share"], 5.0);
        assert_eq!(find("leader")["level"], "low");
        let first: Vec<&str> = budget["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["agent"].as_str().unwrap())
            .collect();
        assert_eq!(
            first,
            ["news", "leader", "strategy", "risk", "review", "trader"]
        );
    }

    #[test]
    fn 색_단계는_0_33과_0_66을_넘을_때_바뀐다() {
        assert_eq!(level(0.33), "low");
        assert_eq!(level(0.34), "mid");
        assert_eq!(level(0.66), "mid");
        assert_eq!(level(0.67), "high");
        assert_eq!(ratio(1, 0.0), 1.0);
        assert_eq!(ratio(0, 0.0), 0.0);
    }

    #[test]
    fn 상한이_없으면_몫과_사용률이_없다() {
        let budget = budget_json(None, 3, &[3, 0, 0, 0, 0, 0]);
        assert_eq!(budget["limit"], Value::Null);
        assert_eq!(budget["reserve"], 0);
        assert_eq!(budget["mode"], "normal");
        assert_eq!(budget["members"][0]["share"], Value::Null);
        assert_eq!(budget["members"][0]["used"], 3);
    }
}
