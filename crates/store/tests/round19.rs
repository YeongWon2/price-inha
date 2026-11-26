#![cfg(test)]
//! 19차: 판 11 — 내 캐릭터(M8) · 담당 의견과 팀장 요약(M9 · D13) · 공지(M10) · 결정 메모(M11).
//!
//! 새 표와 새 함수만 더한다(기존 공개 구조체 · 판 10 옮기기는 그대로). 판 10 · 판 9 파일이
//! 판 11로 열리고 옛 자료가 그대로인지도 본다.

mod common;

use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    AccountKind, Conviction, DecisionMemo, DecisionMemoId, DecisionMemoState, Facing, Judgment,
    MemoSettlement, NOTICE_TEXT_MAX_CHARS, NewDecisionMemo, NewNotice, NewTrade, NoticeAck,
    NoticeAckStatus, NoticeId, PLAYER_DEFAULT_NAME, PLAYER_NAME_MAX_CHARS, Player, PlayerSpot,
    Rationale, RoleOpinion, RoleScore, SCHEMA_VERSION, SourceRef, Stance, Store, StoreError,
    TradeId, TradeOpinions, TradeSide, TradeState, TradeSummary,
};
use rusqlite::types::Value as SqlValue;
use tempfile::TempDir;

const MARKET: &str = "시세 담당";
const RISK: &str = "위험 담당";

// ───────────── 도우미 ─────────────

fn copy_of(name: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let to = dir.path().join("pi.sqlite");
    std::fs::copy(from, &to).unwrap();
    (dir, to)
}

fn raw_user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

fn integrity(path: &Path) -> String {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap()
}

fn new_trade(store: &Store, symbol: &str) -> TradeId {
    store
        .create_trade(NewTrade {
            symbol: symbol.to_string(),
            account: AccountKind::Paper,
            at: t(0),
        })
        .unwrap()
        .id
}

fn score(role: &str, score: i32) -> RoleScore {
    RoleScore {
        role: role.to_string(),
        score,
        reason: format!("{role} 근거"),
    }
}

fn judgment(decided_at: i64, scores: Vec<RoleScore>) -> Judgment {
    Judgment {
        decided_at: t(decided_at),
        rationale: Rationale::default(),
        sources: vec![SourceRef {
            name: "시세".to_string(),
            known_at: t(decided_at - 10),
        }],
        scores,
        consensus_score: 60,
    }
}

fn two_scores() -> Vec<RoleScore> {
    vec![score(MARKET, 30), score(RISK, 0)]
}

fn bullish(role: &str) -> RoleOpinion {
    RoleOpinion {
        role: role.to_string(),
        stance: Some(Stance::Bullish),
        conviction: Some(Conviction::High),
        reason: "거래량이 늘었다".to_string(),
        horizon_days: Some(5),
        abstained: false,
    }
}

fn abstained(role: &str) -> RoleOpinion {
    RoleOpinion {
        role: role.to_string(),
        stance: None,
        conviction: None,
        reason: "자료가 모자라다".to_string(),
        horizon_days: None,
        abstained: true,
    }
}

fn two_opinions() -> Vec<RoleOpinion> {
    vec![bullish(MARKET), abstained(RISK)]
}

fn summary() -> TradeSummary {
    TradeSummary {
        summary: "거래량이 늘었다. 위험 담당은 기권했다. 사기로 한다.".to_string(),
        reversal: "거래량이 다시 줄면 뒤집힌다".to_string(),
    }
}

/// 의견 · 요약과 함께 판단된 매매.
fn opined_trade(store: &Store, symbol: &str, decided_at: i64) -> TradeId {
    let id = new_trade(store, symbol);
    store
        .judge_trade_with_opinions(
            id,
            judgment(decided_at, two_scores()),
            TradeSide::Buy,
            &two_opinions(),
            &summary(),
        )
        .unwrap();
    id
}

fn notice(store: &Store, text: &str, at: i64) -> pi_store::Notice {
    store
        .create_notice(NewNotice {
            text: text.to_string(),
            roles: vec![MARKET.to_string(), RISK.to_string()],
            at: t(at),
        })
        .unwrap()
}

fn memo(store: &Store, trade: TradeId, date: &str) -> DecisionMemo {
    store
        .create_decision_memo(NewDecisionMemo {
            trade,
            date: date.to_string(),
            side: TradeSide::Buy,
            at: t(100),
        })
        .unwrap()
}

fn settled(lesson: &str) -> MemoSettlement {
    MemoSettlement::Settled {
        return_bp: 250,
        excess_bp: Some(-40),
        holding_days: 5,
        lesson: lesson.to_string(),
    }
}

fn rows(path: &Path, table: &str) -> Vec<Vec<SqlValue>> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare(&format!("SELECT rowid, * FROM \"{table}\" ORDER BY rowid"))
        .unwrap();
    let width = stmt.column_count();
    stmt.query_map([], |row| {
        (0..width)
            .map(|i| row.get::<_, SqlValue>(i))
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

/// `rows`가 돌려준 줄의 rowid.
fn rowid(row: &[SqlValue]) -> i64 {
    match row[0] {
        SqlValue::Integer(n) => n,
        ref other => panic!("rowid가 정수가 아니다: {other:?}"),
    }
}

fn columns(path: &Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let cols: Vec<String> = conn
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!cols.is_empty(), "{table} 표가 없다");
    cols
}

/// 제약 오류로 거부되면 `true`, 들어가 버리면 `false`. 그 밖의 오류는 시험 SQL이 틀린 것이다.
/// 우회하는 쪽처럼 외래 키 검사를 끈 연결로 한다(표 스스로 막아야 한다).
fn rejected(path: &Path, sql: &str) -> bool {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    match conn.execute(sql, []) {
        Ok(changed) => {
            assert!(changed > 0, "아무 줄도 고르지 못한 SQL이다: {sql}");
            false
        }
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            true
        }
        Err(other) => panic!("시험하는 SQL이 틀렸다: {sql}\n{other}"),
    }
}

/// 표의 모든 칸을 하나씩 다른 값으로 고쳐 보는 SQL들.
fn update_each_column(path: &Path, table: &str) -> Vec<String> {
    columns(path, table)
        .into_iter()
        .map(|c| {
            format!(
                "UPDATE \"{table}\" SET \"{c}\" = CASE typeof(\"{c}\") \
                 WHEN 'integer' THEN \"{c}\" + 1 WHEN 'text' THEN \"{c}\" || '-x' \
                 WHEN 'null' THEN 1 ELSE \"{c}\" END"
            )
        })
        .collect()
}

/// 지우기 · 모든 칸 고치기 · 같은 줄 다시 넣기가 모두 거부되고 줄이 그대로인지.
fn assert_record_table(path: &Path, table: &str) {
    let before = rows(path, table);
    assert!(!before.is_empty(), "{table}에 줄이 없다");
    let cols = columns(path, table).join(", ");
    let mut sqls = vec![format!("DELETE FROM \"{table}\"")];
    sqls.extend(update_each_column(path, table));
    for verb in ["INSERT OR REPLACE INTO", "REPLACE INTO"] {
        sqls.push(format!(
            "{verb} \"{table}\" ({cols}) SELECT {cols} FROM \"{table}\""
        ));
    }
    for sql in sqls {
        assert!(rejected(path, &sql), "{table}: 들어갔다: {sql}");
        assert_eq!(rows(path, table), before, "{table}: {sql}");
    }
}

// ───────────── 1. 내 캐릭터 (M8) ─────────────

// 검증: M8 T23
#[test]
fn 내_캐릭터의_기본값은_이름_나이고_외형과_자리가_없다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(PLAYER_DEFAULT_NAME, "나");
    assert_eq!(PLAYER_NAME_MAX_CHARS, 50);
    assert_eq!(
        store.player().unwrap(),
        Player {
            name: "나".to_string(),
            look: None,
            spot: None,
            updated_at: None,
        }
    );
}

// 검증: M8 T23
#[test]
fn 이름과_외형을_정하면_덮어쓰고_자리는_그대로다() {
    let (_dir, _path, store) = temp_store();
    store.set_player_spot(3, 4, Facing::Left, t(1)).unwrap();
    store.set_player_profile("영원", "look-a", t(2)).unwrap();
    let now = store.set_player_profile("가격", "look-b", t(3)).unwrap();
    let want = Player {
        name: "가격".to_string(),
        look: Some("look-b".to_string()),
        spot: Some(PlayerSpot {
            x: 3,
            y: 4,
            facing: Facing::Left,
        }),
        updated_at: Some(t(3)),
    };
    assert_eq!(now, want);
    assert_eq!(store.player().unwrap(), want);
}

// 검증: M8 T23
#[test]
fn 이름_0자와_51자와_빈_외형은_거부되고_그대로다() {
    let (_dir, _path, store) = temp_store();
    let fifty = "가".repeat(50);
    let kept = store.set_player_profile(&fifty, "look-a", t(1)).unwrap();
    assert_eq!(kept.name.chars().count(), 50);

    let fifty_one = "가".repeat(51);
    for (name, look) in [("", "look-b"), (fifty_one.as_str(), "look-b"), ("나", "")] {
        let result = store.set_player_profile(name, look, t(2));
        assert!(
            matches!(result, Err(StoreError::InvalidPlayer(_))),
            "{name:?} / {look:?}: {result:?}"
        );
        assert_eq!(store.player().unwrap(), kept, "{name:?} / {look:?}");
    }
}

// 검증: M8 T23
#[test]
fn 자리는_덮어쓰고_이름_외형은_그대로다() {
    let (_dir, _path, store) = temp_store();
    store.set_player_profile("영원", "look-a", t(1)).unwrap();
    store.set_player_spot(1, 2, Facing::Up, t(2)).unwrap();
    for (i, facing) in Facing::ALL.into_iter().enumerate() {
        let i = i as u32;
        let now = store
            .set_player_spot(10 + i, 20 + i, facing, t(3 + i64::from(i)))
            .unwrap();
        assert_eq!(
            now.spot,
            Some(PlayerSpot {
                x: 10 + i,
                y: 20 + i,
                facing,
            })
        );
    }
    let now = store.player().unwrap();
    assert_eq!(now.name, "영원");
    assert_eq!(now.look.as_deref(), Some("look-a"));
    assert_eq!(
        now.spot,
        Some(PlayerSpot {
            x: 13,
            y: 23,
            facing: Facing::Right,
        })
    );
    assert_eq!(now.updated_at, Some(t(6)));
}

// 검증: M8 T23
#[test]
fn 내_캐릭터는_다시_열어도_그대로이고_스냅샷에도_든다() {
    let (dir, path, store) = temp_store();
    store.set_player_profile("영원", "look-a", t(1)).unwrap();
    store.set_player_spot(7, 8, Facing::Down, t(2)).unwrap();
    let want = store.player().unwrap();
    let copy = dir.path().join("copy.sqlite");
    store.snapshot_to(&copy).unwrap();
    drop(store);

    assert_eq!(Store::open(&path).unwrap().player().unwrap(), want);
    assert_eq!(Store::open(&copy).unwrap().player().unwrap(), want);
}

// ───────────── 2. 담당 의견과 팀장 요약 (M9 · D13) ─────────────

// 검증: M9 D13 T23
#[test]
fn 의견과_요약이_판단과_함께_남고_담당_점수는_그대로다() {
    let (_dir, _path, store) = temp_store();
    let id = new_trade(&store, "005930");
    let judged = store
        .judge_trade_with_opinions(
            id,
            judgment(50, two_scores()),
            TradeSide::Buy,
            &two_opinions(),
            &summary(),
        )
        .unwrap();
    assert_eq!(judged.state, TradeState::Judged);
    assert_eq!(judged.scores, two_scores());
    assert_eq!(store.get_trade(id).unwrap(), judged);
    assert_eq!(store.trade_side(id).unwrap(), Some(TradeSide::Buy));
    assert_eq!(
        store.trade_opinions(id).unwrap(),
        TradeOpinions {
            opinions: two_opinions(),
            summary: Some(summary()),
        }
    );
}

// 검증: M9 D13 T23
#[test]
fn 기권은_0점과_따로_남고_방향_확신도가_비어_있다() {
    let (_dir, _path, store) = temp_store();
    let id = opined_trade(&store, "005930", 50);
    let got = store.trade_opinions(id).unwrap();
    let risk = got.opinions.iter().find(|o| o.role == RISK).unwrap();
    assert!(risk.abstained);
    assert_eq!(risk.stance, None);
    assert_eq!(risk.conviction, None);
    // 같은 0점이어도 기권 아닌 중립 의견은 기권이 아니다.
    let other = new_trade(&store, "000660");
    let neutral = RoleOpinion {
        role: RISK.to_string(),
        stance: Some(Stance::Neutral),
        conviction: Some(Conviction::Low),
        reason: "판단 보류".to_string(),
        horizon_days: None,
        abstained: false,
    };
    store
        .judge_trade_with_opinions(
            other,
            judgment(60, two_scores()),
            TradeSide::Buy,
            &[bullish(MARKET), neutral.clone()],
            &summary(),
        )
        .unwrap();
    let got = store.trade_opinions(other).unwrap();
    assert_eq!(got.opinions, vec![bullish(MARKET), neutral]);
}

// 검증: M9 D13 T23
#[test]
fn 모든_방향과_확신도가_그대로_읽힌다() {
    let (_dir, _path, store) = temp_store();
    for stance in Stance::ALL {
        for conviction in Conviction::ALL {
            let id = new_trade(&store, "005930");
            let opinion = RoleOpinion {
                stance: Some(stance),
                conviction: Some(conviction),
                horizon_days: Some(1),
                ..bullish(MARKET)
            };
            store
                .judge_trade_with_opinions(
                    id,
                    judgment(50, vec![score(MARKET, 10)]),
                    TradeSide::Sell,
                    std::slice::from_ref(&opinion),
                    &summary(),
                )
                .unwrap();
            assert_eq!(store.trade_opinions(id).unwrap().opinions, vec![opinion]);
        }
    }
}

// 검증: M9 D13 T23
#[test]
fn 판단이_중간에_실패하면_판단도_의견도_요약도_없다() {
    let (_dir, _path, store) = temp_store();
    let id = new_trade(&store, "005930");
    let before = store.get_trade(id).unwrap();
    let mut late = judgment(50, two_scores());
    late.sources.push(SourceRef {
        name: "늦은 자료".to_string(),
        known_at: t(51),
    });
    let result =
        store.judge_trade_with_opinions(id, late, TradeSide::Buy, &two_opinions(), &summary());
    assert!(
        matches!(result, Err(StoreError::SourceAfterDecision { .. })),
        "{result:?}"
    );
    assert_eq!(store.get_trade(id).unwrap(), before);
    assert_eq!(store.trade_side(id).unwrap(), None);
    assert_eq!(store.trade_opinions(id).unwrap(), TradeOpinions::default());

    // 실패한 뒤에도 같은 매매로 바로 판단된다.
    store
        .judge_trade_with_opinions(
            id,
            judgment(50, two_scores()),
            TradeSide::Buy,
            &two_opinions(),
            &summary(),
        )
        .unwrap();
    assert_eq!(store.trade_opinions(id).unwrap().opinions, two_opinions());
}

// 검증: M9 D13 T23
#[test]
fn 규칙에_맞지_않는_의견이면_판단도_남지_않는다() {
    let (_dir, _path, store) = temp_store();
    let cases: Vec<(&str, Vec<RoleOpinion>)> = vec![
        (
            "기권인데 방향이 있다",
            vec![RoleOpinion {
                stance: Some(Stance::Bearish),
                ..abstained(RISK)
            }],
        ),
        (
            "기권인데 확신도가 있다",
            vec![RoleOpinion {
                conviction: Some(Conviction::Low),
                ..abstained(RISK)
            }],
        ),
        (
            "기권이 아닌데 방향이 없다",
            vec![RoleOpinion {
                stance: None,
                ..bullish(MARKET)
            }],
        ),
        (
            "기권이 아닌데 확신도가 없다",
            vec![RoleOpinion {
                conviction: None,
                ..bullish(MARKET)
            }],
        ),
        (
            "보는 기간 0",
            vec![RoleOpinion {
                horizon_days: Some(0),
                ..bullish(MARKET)
            }],
        ),
        ("점수에 없는 담당", vec![bullish("뉴스 담당")]),
        ("같은 담당 둘", vec![bullish(MARKET), bullish(MARKET)]),
    ];
    for (why, opinions) in cases {
        let id = new_trade(&store, "005930");
        let before = store.get_trade(id).unwrap();
        let result = store.judge_trade_with_opinions(
            id,
            judgment(50, two_scores()),
            TradeSide::Buy,
            &opinions,
            &summary(),
        );
        assert!(
            matches!(result, Err(StoreError::InvalidOpinion(_))),
            "{why}: {result:?}"
        );
        assert_eq!(store.get_trade(id).unwrap(), before, "{why}");
        assert_eq!(store.get_trade(id).unwrap().state, TradeState::Discussing);
        assert_eq!(store.trade_side(id).unwrap(), None, "{why}");
        assert_eq!(
            store.trade_opinions(id).unwrap(),
            TradeOpinions::default(),
            "{why}"
        );
    }
}

// 검증: M9 D13 T23
#[test]
fn 한_번_남은_의견은_다시_쓰지_못하고_그대로다() {
    let (_dir, _path, store) = temp_store();
    let id = opined_trade(&store, "005930", 50);
    let trade = store.get_trade(id).unwrap();
    let kept = store.trade_opinions(id).unwrap();

    let other = TradeSummary {
        summary: "다른 요약".to_string(),
        reversal: "다른 조건".to_string(),
    };
    let result = store.judge_trade_with_opinions(
        id,
        judgment(60, two_scores()),
        TradeSide::Sell,
        &[bullish(RISK)],
        &other,
    );
    assert!(
        matches!(result, Err(StoreError::TransitionDenied { .. })),
        "{result:?}"
    );
    assert_eq!(store.get_trade(id).unwrap(), trade);
    assert_eq!(store.trade_side(id).unwrap(), Some(TradeSide::Buy));
    assert_eq!(store.trade_opinions(id).unwrap(), kept);
}

// 검증: M9 D13 T23
#[test]
fn 의견_없이_판단된_매매는_기록_없음이고_없는_매매는_찾지_못한다() {
    let (_dir, _path, store) = temp_store();
    opined_trade(&store, "000660", 40);
    let plain = new_trade(&store, "005930");
    store
        .judge_trade_with_side(plain, judgment(50, two_scores()), TradeSide::Buy)
        .unwrap();
    assert_eq!(
        store.trade_opinions(plain).unwrap(),
        TradeOpinions::default()
    );
    let discussing = new_trade(&store, "035420");
    assert_eq!(
        store.trade_opinions(discussing).unwrap(),
        TradeOpinions::default()
    );
    assert!(matches!(
        store.trade_opinions(TradeId(9_999)),
        Err(StoreError::TradeNotFound(TradeId(9_999)))
    ));
}

// 검증: M9 D13 T23
#[test]
fn 의견과_요약_표는_직접_sql로도_지우거나_고치거나_다시_넣지_못한다() {
    let (_dir, path, store) = temp_store();
    let id = opined_trade(&store, "005930", 50);
    let kept = store.trade_opinions(id).unwrap();
    drop(store);

    for table in ["role_opinions", "trade_summaries"] {
        assert_record_table(&path, table);
    }
    assert_eq!(
        Store::open(&path).unwrap().trade_opinions(id).unwrap(),
        kept
    );
}

// 검증: M9 D13 T23
#[test]
fn 의견과_요약은_다시_열어도_그대로다() {
    let (_dir, path, store) = temp_store();
    let id = opined_trade(&store, "005930", 50);
    let kept = store.trade_opinions(id).unwrap();
    assert_eq!(kept.opinions.len(), 2);
    drop(store);
    assert_eq!(
        Store::open(&path).unwrap().trade_opinions(id).unwrap(),
        kept
    );
}

// ───────────── 3. 공지 (M10) ─────────────

fn waiting(role: &str) -> NoticeAck {
    NoticeAck {
        role: role.to_string(),
        status: NoticeAckStatus::Waiting,
        answer: None,
        answered_at: None,
    }
}

// 검증: M10 T23
#[test]
fn 공지를_만들면_담당별_확인이_모두_기다림이다() {
    let (_dir, _path, store) = temp_store();
    let made = notice(&store, "내일은 쉰다", 10);
    assert_eq!(made.text, "내일은 쉰다");
    assert_eq!(made.sent_at, t(10));
    assert_eq!(made.acks, vec![waiting(MARKET), waiting(RISK)]);
    assert_eq!(store.get_notice(made.id).unwrap(), made);

    let second = notice(&store, "모레는 연다", 11);
    assert!(second.id > made.id, "번호는 다시 쓰지 않고 커진다");
    assert!(matches!(
        store.get_notice(NoticeId(9_999)),
        Err(StoreError::NoticeNotFound(NoticeId(9_999)))
    ));
}

// 검증: M10 T23
#[test]
fn 공지_글은_500자까지이고_0자와_501자는_거부되고_아무것도_남지_않는다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(NOTICE_TEXT_MAX_CHARS, 500);
    let full = "가".repeat(500);
    let made = notice(&store, &full, 10);
    assert_eq!(made.text.chars().count(), 500);

    let over = "가".repeat(501);
    for text in ["", over.as_str()] {
        let result = store.create_notice(NewNotice {
            text: text.to_string(),
            roles: vec![MARKET.to_string()],
            at: t(20),
        });
        assert!(
            matches!(result, Err(StoreError::InvalidNotice(_))),
            "{}자: {result:?}",
            text.chars().count()
        );
    }
    assert_eq!(store.notices_page(None, 100).unwrap(), vec![made]);
}

// 검증: M10 T23
#[test]
fn 담당별_확인은_기다림에서_한_번만_바뀐다() {
    let (_dir, _path, store) = temp_store();
    for (i, status) in [
        NoticeAckStatus::Answered,
        NoticeAckStatus::Capped,
        NoticeAckStatus::Failed,
    ]
    .into_iter()
    .enumerate()
    {
        let made = notice(&store, "공지", i as i64);
        let after = store
            .answer_notice(made.id, MARKET, status, "답", t(50))
            .unwrap();
        let want_market = NoticeAck {
            role: MARKET.to_string(),
            status,
            answer: Some("답".to_string()),
            answered_at: Some(t(50)),
        };
        assert_eq!(after.acks, vec![want_market.clone(), waiting(RISK)]);
        assert_eq!(store.get_notice(made.id).unwrap(), after);

        // 두 번째는 어떤 상태로도 거부되고 그대로다.
        for again in NoticeAckStatus::ALL {
            let result = store.answer_notice(made.id, MARKET, again, "다른 답", t(60));
            assert!(
                matches!(result, Err(StoreError::NoticeAckLocked { .. })),
                "{status:?} → {again:?}: {result:?}"
            );
        }
        assert_eq!(store.get_notice(made.id).unwrap(), after);
        assert_eq!(after.text, "공지");
    }
}

// 검증: M10 T23
#[test]
fn 기다림으로_답하기_없는_담당_없는_공지는_거부되고_그대로다() {
    let (_dir, _path, store) = temp_store();
    let made = notice(&store, "공지", 10);

    let result = store.answer_notice(made.id, MARKET, NoticeAckStatus::Waiting, "답", t(20));
    assert!(
        matches!(result, Err(StoreError::InvalidNotice(_))),
        "{result:?}"
    );
    let result = store.answer_notice(made.id, "뉴스 담당", NoticeAckStatus::Answered, "답", t(20));
    assert!(
        matches!(result, Err(StoreError::NoticeAckNotFound { .. })),
        "{result:?}"
    );
    let result = store.answer_notice(
        NoticeId(9_999),
        MARKET,
        NoticeAckStatus::Answered,
        "답",
        t(20),
    );
    assert!(
        matches!(result, Err(StoreError::NoticeNotFound(NoticeId(9_999)))),
        "{result:?}"
    );
    assert_eq!(store.get_notice(made.id).unwrap(), made);
}

// 검증: M10 T23
#[test]
fn 공지_쪽_넘기기는_겹침_빠짐_없이_최근부터이고_상한이_100이다() {
    let (_dir, _path, store) = temp_store();
    let made: Vec<NoticeId> = (0..150)
        .map(|i| notice(&store, &format!("공지 {i}"), i).id)
        .collect();

    let first = store.notices_page(None, 1_000).unwrap();
    assert_eq!(first.len(), 100, "상한 100");
    let rest = store
        .notices_page(Some(first.last().unwrap().id), 100)
        .unwrap();
    assert_eq!(rest.len(), 50);
    let all: Vec<NoticeId> = first.iter().chain(rest.iter()).map(|n| n.id).collect();
    let mut want = made.clone();
    want.reverse();
    assert_eq!(all, want);
    assert!(
        store
            .notices_page(Some(rest.last().unwrap().id), 100)
            .unwrap()
            .is_empty()
    );
    assert!(store.notices_page(None, 0).unwrap().is_empty());
}

// 검증: M10 T23
#[test]
fn 공지_시각_범위는_양끝을_넣고_보낸_시각_순서이며_늦은_공지는_빠진다() {
    let (_dir, _path, store) = temp_store();
    let a = notice(&store, "가", 30).id;
    let b = notice(&store, "나", 10).id;
    let c = notice(&store, "다", 20).id;
    let d = notice(&store, "라", 20).id;
    let late = notice(&store, "마", 41).id;

    let ids = |from: i64, to: i64| -> Vec<NoticeId> {
        store
            .notices_between(t(from), t(to))
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect()
    };
    assert_eq!(ids(10, 40), vec![b, c, d, a]);
    assert_eq!(ids(20, 20), vec![c, d]);
    assert_eq!(ids(11, 29), vec![c, d]);
    assert_eq!(ids(0, 41), vec![b, c, d, a, late]);
    assert!(ids(31, 40).is_empty());
}

// 검증: M10 T23
#[test]
fn 공지는_직접_sql로도_지우거나_글을_고치지_못하고_확인은_지우지_못한다() {
    let (_dir, path, store) = temp_store();
    let made = notice(&store, "내일은 쉰다", 10);
    let answered = store
        .answer_notice(made.id, MARKET, NoticeAckStatus::Answered, "알겠다", t(20))
        .unwrap();
    drop(store);

    assert_record_table(&path, "notices");
    let acks = rows(&path, "notice_acks");
    assert!(
        rejected(&path, "DELETE FROM notice_acks"),
        "확인이 지워졌다"
    );
    assert_eq!(rows(&path, "notice_acks"), acks);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_notice(made.id).unwrap(), answered);
}

// 검증: M10 T23
#[test]
fn 공지와_확인은_다시_열어도_그대로다() {
    let (_dir, path, store) = temp_store();
    let made = notice(&store, "내일은 쉰다", 10);
    let answered = store
        .answer_notice(made.id, RISK, NoticeAckStatus::Capped, "상한", t(20))
        .unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_notice(made.id).unwrap(), answered);
    assert_eq!(store.notices_between(t(0), t(100)).unwrap(), vec![answered]);
}

// ───────────── 4. 결정 메모 (M11) ─────────────

// 검증: M11 T23
#[test]
fn 결정_메모는_판단된_매매마다_하나이고_정산_대기로_시작한다() {
    let (_dir, _path, store) = temp_store();
    let id = opined_trade(&store, "005930", 50);
    let made = memo(&store, id, "2026-11-24");
    assert_eq!(
        made,
        DecisionMemo {
            id: made.id,
            trade: id,
            date: "2026-11-24".to_string(),
            symbol: "005930".to_string(),
            side: TradeSide::Buy,
            state: DecisionMemoState::Pending,
            return_bp: None,
            excess_bp: None,
            holding_days: None,
            lesson: None,
            created_at: t(100),
            settled_at: None,
        }
    );
    assert_eq!(store.get_decision_memo(made.id).unwrap(), made);

    let again = store.create_decision_memo(NewDecisionMemo {
        trade: id,
        date: "2026-11-25".to_string(),
        side: TradeSide::Sell,
        at: t(200),
    });
    assert!(
        matches!(again, Err(StoreError::DecisionMemoExists(got)) if got == id),
        "{again:?}"
    );
    assert_eq!(store.decision_memos_page(None, 100).unwrap(), vec![made]);
}

// 검증: M11 T23
#[test]
fn 판단_전_매매와_없는_매매로는_결정_메모를_만들지_못한다() {
    let (_dir, _path, store) = temp_store();
    let judged = opined_trade(&store, "000660", 40);
    let kept = memo(&store, judged, "2026-11-23");
    let discussing = new_trade(&store, "005930");
    let result = store.create_decision_memo(NewDecisionMemo {
        trade: discussing,
        date: "2026-11-24".to_string(),
        side: TradeSide::Buy,
        at: t(100),
    });
    assert!(
        matches!(result, Err(StoreError::TradeNotJudged { id, .. }) if id == discussing),
        "{result:?}"
    );
    let result = store.create_decision_memo(NewDecisionMemo {
        trade: TradeId(9_999),
        date: "2026-11-24".to_string(),
        side: TradeSide::Buy,
        at: t(100),
    });
    assert!(
        matches!(result, Err(StoreError::TradeNotFound(TradeId(9_999)))),
        "{result:?}"
    );
    assert_eq!(store.decision_memos_page(None, 100).unwrap(), vec![kept]);
    assert!(matches!(
        store.get_decision_memo(DecisionMemoId(9_999)),
        Err(StoreError::DecisionMemoNotFound(DecisionMemoId(9_999)))
    ));
}

// 검증: M11 T23
#[test]
fn 정산은_한_번만_되고_정산됨에만_교훈이_있다() {
    let (_dir, _path, store) = temp_store();
    let a = memo(&store, opined_trade(&store, "005930", 50), "2026-11-20");
    let b = memo(&store, opined_trade(&store, "000660", 50), "2026-11-20");

    let done = store
        .settle_decision_memo(
            a.id,
            settled("거래량 신호가 맞았다. 다음에도 본다."),
            t(300),
        )
        .unwrap();
    assert_eq!(done.state, DecisionMemoState::Settled);
    assert_eq!(done.return_bp, Some(250));
    assert_eq!(done.excess_bp, Some(-40));
    assert_eq!(done.holding_days, Some(5));
    assert_eq!(
        done.lesson.as_deref(),
        Some("거래량 신호가 맞았다. 다음에도 본다.")
    );
    assert_eq!(done.settled_at, Some(t(300)));

    let missed = store
        .settle_decision_memo(b.id, MemoSettlement::Unsettleable, t(301))
        .unwrap();
    assert_eq!(missed.state, DecisionMemoState::Unsettleable);
    assert_eq!(missed.lesson, None);
    assert_eq!(missed.return_bp, None);
    assert_eq!(missed.excess_bp, None);
    assert_eq!(missed.holding_days, None);
    assert_eq!(missed.settled_at, Some(t(301)));

    for (id, kept) in [(a.id, &done), (b.id, &missed)] {
        for outcome in [settled("다시"), MemoSettlement::Unsettleable] {
            let result = store.settle_decision_memo(id, outcome, t(400));
            assert!(
                matches!(result, Err(StoreError::DecisionMemoLocked(got)) if got == id),
                "{result:?}"
            );
        }
        assert_eq!(&store.get_decision_memo(id).unwrap(), kept);
    }
    let result = store.settle_decision_memo(DecisionMemoId(9_999), settled("x"), t(400));
    assert!(
        matches!(
            result,
            Err(StoreError::DecisionMemoNotFound(DecisionMemoId(9_999)))
        ),
        "{result:?}"
    );
}

// 검증: M11 T23
#[test]
fn 교훈이_빈_정산은_거부되고_정산_대기_그대로다() {
    let (_dir, _path, store) = temp_store();
    let made = memo(&store, opined_trade(&store, "005930", 50), "2026-11-20");
    let result = store.settle_decision_memo(made.id, settled(""), t(300));
    assert!(
        matches!(result, Err(StoreError::InvalidDecisionMemo(_))),
        "{result:?}"
    );
    assert_eq!(store.get_decision_memo(made.id).unwrap(), made);

    // 초과수익이 없는 정산은 된다.
    let done = store
        .settle_decision_memo(
            made.id,
            MemoSettlement::Settled {
                return_bp: -120,
                excess_bp: None,
                holding_days: 3,
                lesson: "기준 지수가 없었다.".to_string(),
            },
            t(301),
        )
        .unwrap();
    assert_eq!(done.return_bp, Some(-120));
    assert_eq!(done.excess_bp, None);
}

// 검증: M11 T23
#[test]
fn 정산_대기_목록은_기준_날짜까지이고_날짜_순서이며_정산된_것은_빠진다() {
    let (_dir, _path, store) = temp_store();
    let late = memo(&store, opined_trade(&store, "000001", 50), "2026-11-22");
    let mid = memo(&store, opined_trade(&store, "000002", 50), "2026-11-21");
    let early = memo(&store, opined_trade(&store, "000003", 50), "2026-11-20");
    let early2 = memo(&store, opined_trade(&store, "000004", 50), "2026-11-20");

    let ids = |on: &str| -> Vec<DecisionMemoId> {
        store
            .decision_memos_pending(on)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect()
    };
    assert_eq!(ids("2026-11-21"), vec![early.id, early2.id, mid.id]);
    assert_eq!(
        ids("2026-11-30"),
        vec![early.id, early2.id, mid.id, late.id]
    );
    assert!(ids("2026-11-19").is_empty());

    store
        .settle_decision_memo(early.id, settled("맞았다."), t(300))
        .unwrap();
    store
        .settle_decision_memo(mid.id, MemoSettlement::Unsettleable, t(301))
        .unwrap();
    assert_eq!(ids("2026-11-30"), vec![early2.id, late.id]);
}

// 검증: M11 T23
#[test]
fn 결정_메모_쪽_넘기기는_최근부터이고_겹치지_않는다() {
    let (_dir, _path, store) = temp_store();
    let made: Vec<DecisionMemoId> = (0..5)
        .map(|i| {
            memo(
                &store,
                opined_trade(&store, &format!("00000{i}"), 50),
                "2026-11-20",
            )
            .id
        })
        .collect();
    let first = store.decision_memos_page(None, 2).unwrap();
    let second = store
        .decision_memos_page(Some(first.last().unwrap().id), 2)
        .unwrap();
    let third = store
        .decision_memos_page(Some(second.last().unwrap().id), 2)
        .unwrap();
    let all: Vec<DecisionMemoId> = first
        .iter()
        .chain(&second)
        .chain(&third)
        .map(|m| m.id)
        .collect();
    let mut want = made;
    want.reverse();
    assert_eq!(all, want);
    assert!(store.decision_memos_page(None, 0).unwrap().is_empty());
}

// 검증: M11 T23
#[test]
fn 결정_메모는_직접_sql로도_지우지_못하고_다시_열어도_그대로다() {
    let (_dir, path, store) = temp_store();
    let a = memo(&store, opined_trade(&store, "005930", 50), "2026-11-20");
    let done = store
        .settle_decision_memo(a.id, settled("맞았다."), t(300))
        .unwrap();
    let b = memo(&store, opined_trade(&store, "000660", 50), "2026-11-21");
    drop(store);

    let before = rows(&path, "decision_memos");
    assert_eq!(before.len(), 2);
    assert!(rejected(&path, "DELETE FROM decision_memos"), "지워졌다");
    assert!(
        rejected(
            &path,
            &format!(
                "DELETE FROM decision_memos WHERE rowid = {}",
                rowid(&before[1])
            )
        ),
        "골라 지우기가 들어갔다"
    );
    assert_eq!(rows(&path, "decision_memos"), before);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_decision_memo(a.id).unwrap(), done);
    assert_eq!(store.get_decision_memo(b.id).unwrap(), b);
}

// ───────────── 5. 판 11 옮기기 ─────────────

// 검증: M8 M9 M10 M11 D13 T23
#[test]
fn 새_저장소는_판_11이고_무결성이_맞다() {
    let (_dir, path, store) = temp_store();
    assert_eq!(SCHEMA_VERSION, 12);
    assert_eq!(store.info().unwrap().schema_version, 12);
    drop(store);
    assert_eq!(raw_user_version(&path), 12);
    assert_eq!(integrity(&path), "ok");
    for table in [
        "role_opinions",
        "trade_summaries",
        "notices",
        "notice_acks",
        "decision_memos",
    ] {
        columns(&path, table);
    }
}

// 검증: M8 M9 M10 M11 D13 T23
#[test]
fn 판_10_파일이_판_11로_열리고_옛_자료가_그대로다() {
    let (_dir, path) = copy_of("v10.db");
    assert_eq!(raw_user_version(&path), 10);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 12);

    // 판 10에서 판단된 매매(담당 점수 둘)와 논의중 매매가 그대로다. 의견은 기록 없음이다.
    let old = store.get_trade(TradeId(1)).unwrap();
    assert_eq!(old.symbol, "005930");
    assert_eq!(old.state, TradeState::Judged);
    assert_eq!(old.decided_at, Some(t(50)));
    assert_eq!(
        old.scores,
        vec![
            RoleScore {
                role: MARKET.to_string(),
                score: 30,
                reason: "거래량 증가".to_string(),
            },
            RoleScore {
                role: RISK.to_string(),
                score: -10,
                reason: "변동성".to_string(),
            },
        ]
    );
    assert_eq!(store.trade_side(TradeId(1)).unwrap(), Some(TradeSide::Buy));
    assert_eq!(
        store.trade_opinions(TradeId(1)).unwrap(),
        TradeOpinions::default()
    );
    assert_eq!(
        store.get_trade(TradeId(2)).unwrap().state,
        TradeState::Discussing
    );
    assert_eq!(
        store
            .office_looks()
            .unwrap()
            .get(MARKET)
            .map(String::as_str),
        Some("look-a")
    );

    // 새 표는 비어 있고 내 캐릭터는 기본값이다.
    assert_eq!(store.player().unwrap().name, PLAYER_DEFAULT_NAME);
    assert_eq!(store.player().unwrap().updated_at, None);
    assert!(store.notices_page(None, 100).unwrap().is_empty());
    assert!(store.decision_memos_page(None, 100).unwrap().is_empty());

    // 올라간 뒤 새 길이 바로 된다: 옛 판단 매매에 결정 메모, 공지, 내 캐릭터.
    let made = memo(&store, TradeId(1), "2026-11-24");
    assert_eq!(made.symbol, "005930");
    notice(&store, "판 11", 10);
    store.set_player_profile("영원", "look-a", t(1)).unwrap();
    drop(store);

    assert_eq!(raw_user_version(&path), 12);
    assert_eq!(integrity(&path), "ok");
    let again = Store::open(&path).unwrap();
    assert_eq!(again.info().unwrap().schema_version, 12);
    assert_eq!(again.get_trade(TradeId(1)).unwrap(), old);
    assert_eq!(again.get_decision_memo(made.id).unwrap(), made);
    assert_eq!(again.player().unwrap().name, "영원");
}

// 검증: M8 M9 M10 M11 D13 T23
#[test]
fn 판_9_파일도_판_11로_열리고_새_표가_빈다() {
    let (_dir, path) = copy_of("v9.db");
    assert_eq!(raw_user_version(&path), 9);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 12);
    assert_eq!(store.player().unwrap().name, PLAYER_DEFAULT_NAME);
    assert!(store.notices_page(None, 100).unwrap().is_empty());
    assert!(store.decision_memos_page(None, 100).unwrap().is_empty());
    drop(store);
    assert_eq!(raw_user_version(&path), 12);
    assert_eq!(integrity(&path), "ok");
}
