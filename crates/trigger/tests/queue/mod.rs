//! 깨우기 대기열(원본: OpenClaw 시스템 이벤트 대기열).

use pi_trigger::{LANE_CAPACITY, Lane, PushOutcome, QueuedItem, Target, WakeQueue};

fn item(lane: Lane, key: Option<&str>, text: &str, replace: bool) -> QueuedItem {
    QueuedItem {
        lane,
        key: key.map(str::to_string),
        text: text.to_string(),
        replace,
    }
}

fn team() -> Target {
    Target::Team
}

// 검증: F2
#[test]
fn 키가_없으면_맨_뒤_항목과_글이_같을_때만_합친다() {
    let mut queue = WakeQueue::new();
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, None, "가", false)),
        PushOutcome::Queued
    );
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, None, "가", false)),
        PushOutcome::Duplicate
    );
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, None, "나", false)),
        PushOutcome::Queued
    );
    // "가"는 맨 뒤가 아니므로 다시 넣는다.
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, None, "가", false)),
        PushOutcome::Queued
    );
    assert_eq!(queue.len(&team(), Lane::Wake), 3);
}

// 검증: F2
#[test]
fn 키가_있으면_같은_키_같은_글이_어디에든_있을_때_합친다() {
    let mut queue = WakeQueue::new();
    let key = Some("price:005930:above");
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, key, "넘음 100", false)),
        PushOutcome::Queued
    );
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, Some("other"), "다른 것", false)),
        PushOutcome::Queued
    );
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, key, "넘음 100", false)),
        PushOutcome::Duplicate
    );
    // 같은 키라도 글이 다르면 넣는다.
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, key, "넘음 101", false)),
        PushOutcome::Queued
    );
    assert_eq!(queue.len(&team(), Lane::Wake), 3);
}

// 검증: F2
#[test]
fn 교체는_같은_키의_대기_항목을_지우고_새것을_넣는다() {
    let mut queue = WakeQueue::new();
    let key = Some("price:005930");
    queue.push(&team(), item(Lane::Wake, key, "넘음 100", false));
    queue.push(&team(), item(Lane::Wake, key, "넘음 101", false));
    queue.push(&team(), item(Lane::Wake, Some("x"), "남는 것", false));
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, key, "넘음 102", true)),
        PushOutcome::Replaced
    );
    let texts: Vec<String> = queue
        .drain(&team())
        .into_iter()
        .map(|item| item.text)
        .collect();
    assert_eq!(texts, vec!["남는 것".to_string(), "넘음 102".to_string()]);
    assert!(queue.is_empty(&team()));
}

// 검증: F2
#[test]
fn 교체할_것이_같은_글_하나뿐이면_아무것도_하지_않는다() {
    let mut queue = WakeQueue::new();
    let key = Some("k");
    queue.push(&team(), item(Lane::Wake, key, "같음", false));
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, key, "같음", true)),
        PushOutcome::Duplicate
    );
    assert_eq!(queue.len(&team(), Lane::Wake), 1);
}

// 검증: F2
#[test]
fn 키_없는_교체는_받지_않는다() {
    let mut queue = WakeQueue::new();
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, None, "가", true)),
        PushOutcome::Invalid
    );
    assert!(queue.is_empty(&team()));
}

// 검증: F2
#[test]
fn 알림과_깨우기는_따로_20개씩이고_가득_차면_넣지_않는다() {
    assert_eq!(LANE_CAPACITY, 20);
    let mut queue = WakeQueue::new();
    for n in 0..20 {
        assert_eq!(
            queue.push(
                &team(),
                item(Lane::Notice, None, &format!("알림 {n}"), false)
            ),
            PushOutcome::Queued
        );
    }
    assert_eq!(
        queue.push(&team(), item(Lane::Notice, None, "알림 넘침", false)),
        PushOutcome::Full
    );
    for n in 0..20 {
        assert_eq!(
            queue.push(
                &team(),
                item(Lane::Wake, None, &format!("깨우기 {n}"), false)
            ),
            PushOutcome::Queued
        );
    }
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, None, "깨우기 넘침", false)),
        PushOutcome::Full
    );
    assert_eq!(queue.len(&team(), Lane::Notice), 20);
    assert_eq!(queue.len(&team(), Lane::Wake), 20);
    // 다른 대상은 따로 센다.
    let member = Target::Member("news".to_string());
    assert_eq!(
        queue.push(&member, item(Lane::Wake, None, "깨우기", false)),
        PushOutcome::Queued
    );
}

// 검증: F2
#[test]
fn 가득_찬_줄에서도_교체는_지운_뒤에_센다() {
    let mut queue = WakeQueue::new();
    for n in 0..19 {
        queue.push(
            &team(),
            item(Lane::Wake, None, &format!("깨우기 {n}"), false),
        );
    }
    queue.push(&team(), item(Lane::Wake, Some("k"), "옛것", false));
    assert_eq!(queue.len(&team(), Lane::Wake), 20);
    assert_eq!(
        queue.push(&team(), item(Lane::Wake, Some("k"), "새것", true)),
        PushOutcome::Replaced
    );
    assert_eq!(queue.len(&team(), Lane::Wake), 20);
}

// 검증: F2
#[test]
fn 꺼내면_넣은_순서대로_나오고_그_대상만_비운다() {
    let mut queue = WakeQueue::new();
    let member = Target::Member("price".to_string());
    queue.push(&team(), item(Lane::Notice, None, "1", false));
    queue.push(&team(), item(Lane::Wake, None, "2", false));
    queue.push(&member, item(Lane::Wake, None, "다른 대상", false));
    queue.push(&team(), item(Lane::Notice, None, "3", false));
    let texts: Vec<String> = queue
        .drain(&team())
        .into_iter()
        .map(|item| item.text)
        .collect();
    assert_eq!(texts, vec!["1", "2", "3"]);
    assert!(queue.is_empty(&team()));
    assert_eq!(queue.len(&member, Lane::Wake), 1);
    assert!(queue.drain(&team()).is_empty());
}
