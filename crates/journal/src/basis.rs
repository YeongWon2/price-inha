//! 매매 근거 읽기(C6 · V18)와 근거가 다 있는지 판정(V16).

use pi_store::{Amount, Rationale, RationaleField, Trade, UnixMillis};

/// 비어 있는 칸에 쓰는 쉬운 말.
const EMPTY: &str = "비어 있음";

/// 쉬운 말 근거 요약(C6 · V18). AI를 쓰지 않는 고정 문장 틀이다(결정 2026-11-16 28번).
///
/// - 같은 매매 값이면 언제나 같은 글자를 준다.
/// - 근거 칸(가설 · 계기 · 들어간 값 · 손절값 · 목표값 · 기대값), 담당별 점수와 그 까닭, 합의 점수,
///   쓴 자료 이름과 알게 된 시각이 들어간다. 시각은 한국 시간 `YYYY-MM-DD HH:MM`이다.
/// - 빈 칸은 "비어 있음"처럼 쉬운 말로 적는다(`None` · `Some(` 같은 개발 글자가 새지 않는다).
pub fn plain_summary(trade: &Trade) -> String {
    let rationale = &trade.rationale;
    let mut lines = vec![
        format!("종목 {}", trade.symbol),
        format!("가설: {}", text_or_empty(rationale.hypothesis.as_deref())),
        format!("계기: {}", text_or_empty(rationale.trigger.as_deref())),
        format!("들어간 값: {}", amount_or_empty(rationale.entry)),
        format!("손절값: {}", amount_or_empty(rationale.stop)),
        format!("목표값: {}", amount_or_empty(rationale.target)),
        format!("기대값: {}", amount_or_empty(rationale.expected_value)),
        "담당별 점수".to_string(),
    ];
    if trade.scores.is_empty() {
        lines.push(format!("- {EMPTY}"));
    }
    for score in &trade.scores {
        lines.push(format!(
            "- {} {}점: {}",
            score.role,
            score.score,
            text_or_empty(Some(&score.reason))
        ));
    }
    lines.push(match trade.consensus_score {
        Some(score) => format!("합의 점수: {score}점"),
        None => format!("합의 점수: {EMPTY}"),
    });
    lines.push("쓴 자료".to_string());
    if trade.sources.is_empty() {
        lines.push(format!("- {EMPTY}"));
    }
    for source in &trade.sources {
        lines.push(format!(
            "- {} (알게 된 시각 {})",
            source.name,
            format_kst(source.known_at)
        ));
    }
    lines.join("\n")
}

/// 비어 있는 근거 칸들(V16). 칸이 `None`이거나 글자 칸이 비었거나 공백뿐이면 빈 것이다.
/// [`RationaleField`] 선언 순서(가설 · 계기 · 들어간 값 · 손절값 · 목표값 · 기대값)로 준다.
pub fn missing_fields(rationale: &Rationale) -> Vec<RationaleField> {
    let filled = |text: &Option<String>| text.as_deref().is_some_and(|t| !t.trim().is_empty());
    let checks = [
        (RationaleField::Hypothesis, filled(&rationale.hypothesis)),
        (RationaleField::Trigger, filled(&rationale.trigger)),
        (RationaleField::Entry, rationale.entry.is_some()),
        (RationaleField::Stop, rationale.stop.is_some()),
        (RationaleField::Target, rationale.target.is_some()),
        (
            RationaleField::ExpectedValue,
            rationale.expected_value.is_some(),
        ),
    ];
    checks
        .into_iter()
        .filter(|(_, present)| !present)
        .map(|(field, _)| field)
        .collect()
}

fn text_or_empty(text: Option<&str>) -> String {
    match text {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => EMPTY.to_string(),
    }
}

fn amount_or_empty(amount: Option<Amount>) -> String {
    match amount {
        Some(Amount(value)) => with_commas(value),
        None => EMPTY.to_string(),
    }
}

/// 천 단위 쉼표를 넣은 글자(`70,000원`).
fn with_commas(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{out}원")
}

/// 한국 시간(UTC+9) `YYYY-MM-DD HH:MM`.
fn format_kst(at: UnixMillis) -> String {
    const KST_OFFSET_SECS: i64 = 9 * 3600;
    let secs = at.0.div_euclid(1000) + KST_OFFSET_SECS;
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// 1970-01-01부터 센 날수를 (년, 월, 일)로(그레고리력).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
