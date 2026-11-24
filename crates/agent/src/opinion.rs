//! 담당 의견 칸과 팀장 요약 칸 읽기(11차, D13 · R6-4 · R6-5 · M9).
//!
//! - 점수 턴의 답 객체(`score` 칸이 있는 것)에 `stance`(`bullish` · `bearish` · `neutral`),
//!   `conviction`(`low` · `medium` · `high`), `opinion_reason`(한 줄 이유), `horizon_days`(거래일 수,
//!   생략 가능)가 오면 그 담당의 의견이 된다. 방향 · 확신도 · 이유 가운데 하나라도 없거나 틀리면
//!   의견은 없다(점수는 따로 받는다). 보는 기간이 0이거나 1 이상 정수가 아니면 기간만 비운다.
//! - 팀장 판단 답 객체(`decision` 칸이 있는 것)에 `summary` · `reversal`이 둘 다 공백 아닌 글이면 요약이 된다.
//! - 글은 앞뒤 공백을 떼고 가린다(E38).

use pi_ai::Redactor;
use pi_store::{Conviction, RoleOpinion, Stance, TradeSummary};
use serde_json::{Map, Value};

use crate::action::find_object_with;
use crate::role::Role;

/// 점수 턴의 답에서 그 담당의 의견을 읽는다. 새 칸이 온전하지 않으면 `None`(기록 없음).
pub(crate) fn parse_opinion(
    text: &str,
    role: Role,
    redactor: &dyn Redactor,
) -> Option<RoleOpinion> {
    let object = find_object_with(text, "score")?;
    let stance = match word(&object, "stance")?.as_str() {
        "bullish" => Stance::Bullish,
        "bearish" => Stance::Bearish,
        "neutral" => Stance::Neutral,
        _ => return None,
    };
    let conviction = match word(&object, "conviction")?.as_str() {
        "low" => Conviction::Low,
        "medium" => Conviction::Medium,
        "high" => Conviction::High,
        _ => return None,
    };
    let reason = filled(&object, "opinion_reason")?;
    Some(RoleOpinion {
        role: role.id().to_string(),
        stance: Some(stance),
        conviction: Some(conviction),
        reason: redactor.redact(&reason),
        horizon_days: horizon(&object),
        abstained: false,
    })
}

/// 팀장 판단 답에서 요약 · 뒤집힐 조건을 읽는다. 둘 중 하나라도 없거나 공백뿐이면 `None`.
pub(crate) fn parse_summary(text: &str, redactor: &dyn Redactor) -> Option<TradeSummary> {
    let object = find_object_with(text, "decision")?;
    let summary = filled(&object, "summary")?;
    let reversal = filled(&object, "reversal")?;
    Some(TradeSummary {
        summary: redactor.redact(&summary),
        reversal: redactor.redact(&reversal),
    })
}

/// 글 칸을 앞뒤 공백을 떼고 소문자로 읽는다.
fn word(object: &Map<String, Value>, key: &str) -> Option<String> {
    Some(object.get(key)?.as_str()?.trim().to_ascii_lowercase())
}

/// 공백 아닌 글 칸(앞뒤 공백을 뗀 것).
fn filled(object: &Map<String, Value>, key: &str) -> Option<String> {
    let text = object.get(key)?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// 보는 기간: 1 이상 정수만 된다. 0 · 음수 · 정수 아님 · 너무 큼은 비운다.
fn horizon(object: &Map<String, Value>) -> Option<u32> {
    let days = u32::try_from(object.get("horizon_days")?.as_u64()?).ok()?;
    (days > 0).then_some(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plain;

    impl Redactor for Plain {
        fn redact(&self, text: &str) -> String {
            text.to_string()
        }
    }

    fn read(text: &str) -> Option<RoleOpinion> {
        parse_opinion(text, Role::Risk, &Plain)
    }

    #[test]
    fn 대소문자와_공백은_무시하고_기간이_틀리면_기간만_비운다() {
        let text = "{\"score\": 1, \"reason\": \"r\", \"stance\": \" Bullish \", \
                    \"conviction\": \"HIGH\", \"opinion_reason\": \" 이유 \", \"horizon_days\": 0}";
        let got = read(text).unwrap();
        assert_eq!(got.stance, Some(Stance::Bullish));
        assert_eq!(got.conviction, Some(Conviction::High));
        assert_eq!(got.reason, "이유");
        assert_eq!(got.horizon_days, None);
        for bad in ["-3", "2.5", "\"5\"", "null", "99999999999"] {
            let text = text.replace("\"horizon_days\": 0", &format!("\"horizon_days\": {bad}"));
            assert_eq!(read(&text).unwrap().horizon_days, None, "{bad}");
        }
    }

    #[test]
    fn 이유가_비거나_없으면_의견이_없다() {
        let base =
            "{\"score\": 1, \"reason\": \"r\", \"stance\": \"neutral\", \"conviction\": \"low\"";
        assert!(read(&format!("{base}}}")).is_none());
        assert!(read(&format!("{base}, \"opinion_reason\": \"  \"}}")).is_none());
        assert!(read(&format!("{base}, \"opinion_reason\": 3}}")).is_none());
    }

    #[test]
    fn 요약은_두_칸이_모두_글이어야_한다() {
        let ok = "{\"decision\": \"buy\", \"summary\": \" 요약 \", \"reversal\": \"조건\"}";
        let got = parse_summary(ok, &Plain).unwrap();
        assert_eq!(got.summary, "요약");
        assert!(
            parse_summary(
                "{\"decision\": \"buy\", \"summary\": \"a\", \"reversal\": 1}",
                &Plain
            )
            .is_none()
        );
    }
}
