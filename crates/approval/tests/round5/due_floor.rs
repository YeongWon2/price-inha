//! `unhandled_due`는 처리 기록이 남은 닫힌 요청을 다시 읽지 않아도 되게 바뀐다(몇 달 뒤 비용). 그래도 돌려주는
//! 것은 그대로다: 먼저 만든 요청이 나중에 닫혀도 나오고, 다른 관리자(실행기 · 다시 켠 서버)가 남긴 처리 기록은
//! 빠지고, 처리 기록이 쌓인 뒤 새로 닫힌 요청도 나온다. 닫힌 순서도 그대로다.

use pi_approval::{DueAction, DueHandling, Notice};

use crate::common::{MIN, T0, buy, close_due_at, env, reopen, requested, stop_loss};

// 검증: E22 M5
#[test]
fn 먼저_만든_요청이_나중에_닫혀도_처리_기록_뒤에_나온다() {
    let e = env();
    // 사기(5분 기다림)를 먼저, 손절(1분 기다림)을 나중에 만든다: 번호는 사기가 작지만 손절이 먼저 닫힌다.
    let early = requested(&e, buy("early"), Some(Notice::Sent));
    let late = requested(&e, stop_loss("late"), Some(Notice::Sent));
    close_due_at(&e, T0 + MIN);
    let first = e.approvals.unhandled_due().unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].number, late.number);
    e.approvals
        .record_due_handling(&late.number, DueHandling::TeamKept)
        .unwrap();
    assert!(e.approvals.unhandled_due().unwrap().is_empty());

    // 번호가 작은 사기가 이제 닫힌다: 같은 관리자에서도 나온다.
    close_due_at(&e, T0 + 5 * MIN);
    let second = e.approvals.unhandled_due().unwrap();
    assert_eq!(second.len(), 1, "{second:?}");
    assert_eq!(second[0].number, early.number);
    assert_eq!(second[0].action, DueAction::ReduceWithinLimit);
}

// 검증: E22 M5
#[test]
fn 처리_기록이_많이_쌓인_뒤에도_새로_닫힌_것만_닫힌_순서로_준다() {
    let e = env();
    let mut handled = Vec::new();
    for round in 0..120 {
        let r = requested(&e, stop_loss(&format!("old-{round}")), Some(Notice::Sent));
        handled.push(r.number);
    }
    close_due_at(&e, T0 + MIN);
    assert_eq!(e.approvals.unhandled_due().unwrap().len(), handled.len());
    for number in &handled {
        e.approvals
            .record_due_handling(number, DueHandling::TeamKept)
            .unwrap();
    }
    assert!(e.approvals.unhandled_due().unwrap().is_empty());

    // 새 요청 둘: 손절이 먼저 닫히고 사기가 나중에 닫힌다.
    let new_buy = requested(&e, buy("new-buy"), Some(Notice::Sent));
    let new_stop = requested(&e, stop_loss("new-stop"), Some(Notice::Sent));
    close_due_at(&e, T0 + 2 * MIN);
    close_due_at(&e, T0 + 6 * MIN);
    let left: Vec<_> = e
        .approvals
        .unhandled_due()
        .unwrap()
        .into_iter()
        .map(|due| due.number)
        .collect();
    assert_eq!(left, vec![new_stop.number, new_buy.number], "닫힌 순서");
}

// 검증: E22 M5
#[test]
fn 다른_관리자가_남긴_처리_기록은_빠지고_다시_켜도_같다() {
    let e = env();
    let a = requested(&e, stop_loss("a"), Some(Notice::Sent));
    let b = requested(&e, stop_loss("b"), Some(Notice::Sent));
    close_due_at(&e, T0 + MIN);
    // 실행기처럼 같은 저장소를 읽는 다른 관리자가 먼저 한 번 읽어 둔다.
    let reader = reopen(&e.path, e.clock.clone());
    assert_eq!(reader.unhandled_due().unwrap().len(), 2);

    e.approvals
        .record_due_handling(&a.number, DueHandling::TeamKept)
        .unwrap();
    let left = reader.unhandled_due().unwrap();
    assert_eq!(left.len(), 1, "다른 관리자가 남긴 기록이 보인다: {left:?}");
    assert_eq!(left[0].number, b.number);

    e.approvals
        .record_due_handling(&b.number, DueHandling::TeamKept)
        .unwrap();
    assert!(reader.unhandled_due().unwrap().is_empty());
    assert!(
        reopen(&e.path, e.clock.clone())
            .unhandled_due()
            .unwrap()
            .is_empty()
    );

    // 그 뒤 새로 닫힌 요청은 읽어 둔 관리자에게도 나온다.
    let c = requested(&e, stop_loss("c"), Some(Notice::Sent));
    close_due_at(&e, T0 + 2 * MIN);
    let late = reader.unhandled_due().unwrap();
    assert_eq!(late.len(), 1, "{late:?}");
    assert_eq!(late[0].number, c.number);
}
