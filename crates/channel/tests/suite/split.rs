//! 긴 글 나누기(V87). 글자 수는 유니코드 글자 기준.

use pi_channel::{TELEGRAM_TEXT_MAX, split_text};

/// 조각마다 한도 안 · 빈 조각 없음 · 글자를 버리거나 바꾸지 않음(자른 줄바꿈만 빠질 수 있다).
fn assert_well_formed(text: &str, pieces: &[String], max: usize) {
    for p in pieces {
        assert!(!p.is_empty(), "빈 조각");
        assert!(p.chars().count() <= max, "한도 넘음: {}", p.chars().count());
    }
    assert_eq!(pieces.concat().replace('\n', ""), text.replace('\n', ""));
}

// 검증: V87
#[test]
fn 한도_안이면_글_그대로_한_조각이다() {
    assert_eq!(
        split_text("안녕\n세계", 4000),
        vec!["안녕\n세계".to_string()]
    );
    assert!(split_text("", 4000).is_empty());
}

// 검증: V87
#[test]
fn 한글_한도는_바이트가_아니라_글자로_센다() {
    let exact = "가".repeat(TELEGRAM_TEXT_MAX);
    assert_eq!(split_text(&exact, TELEGRAM_TEXT_MAX), vec![exact.clone()]);
    let over = "가".repeat(TELEGRAM_TEXT_MAX + 1);
    let pieces = split_text(&over, TELEGRAM_TEXT_MAX);
    assert_eq!(pieces.len(), 2);
    assert_eq!(pieces[0].chars().count(), TELEGRAM_TEXT_MAX);
    assert_eq!(pieces.concat(), over);
}

// 검증: V87
#[test]
fn 이모지도_한_글자로_센다() {
    let exact = "😀".repeat(TELEGRAM_TEXT_MAX);
    assert_eq!(split_text(&exact, TELEGRAM_TEXT_MAX).len(), 1);
    let over = "😀".repeat(TELEGRAM_TEXT_MAX + 1);
    let pieces = split_text(&over, TELEGRAM_TEXT_MAX);
    assert_eq!(pieces.len(), 2);
    assert_well_formed(&over, &pieces, TELEGRAM_TEXT_MAX);
}

// 검증: V87
#[test]
fn 줄_경계에서_자른다() {
    let text = format!("{}\n{}", "가".repeat(10), "나".repeat(10));
    assert_eq!(
        split_text(&text, 15),
        vec!["가".repeat(10), "나".repeat(10)]
    );
}

// 검증: V87
#[test]
fn 줄이_짧으면_조각을_줄바꿈으로_이으면_원래_글이다() {
    let text = (0..300)
        .map(|i| format!("{i}번 줄 한글 😀"))
        .collect::<Vec<_>>()
        .join("\n");
    let pieces = split_text(&text, 100);
    assert!(pieces.len() > 1);
    assert_well_formed(&text, &pieces, 100);
    assert_eq!(pieces.join("\n"), text);
}

// 검증: V87
#[test]
fn 한도보다_긴_줄은_한도에서_자르고_글자를_버리지_않는다() {
    let text = format!("앞\n{}\n뒤", "가".repeat(25));
    let pieces = split_text(&text, 10);
    assert!(pieces.len() >= 3);
    assert_well_formed(&text, &pieces, 10);
}
