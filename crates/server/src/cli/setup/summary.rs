//! 구역마다의 결과와 끝 요약 표. 까닭에 비밀값을 싣지 않는다(받은 키를 되돌려 보이지 않는다).

use super::prompt::Prompter;
use super::{
    SETUP_DOCTOR_HINT, SETUP_DONE, SETUP_FAILED, SETUP_SKIPPED, SETUP_SUMMARY_TITLE, SetupSection,
};

/// 답이 끝났을 때의 까닭.
pub(crate) const ANSWERS_ENDED: &str = "답이 끝남";

/// 구역 하나의 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Result3 {
    Done,
    Skipped,
    Failed,
}

/// 구역 하나의 결과와 까닭.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub(crate) result: Result3,
    pub(crate) reason: String,
}

impl Outcome {
    pub(crate) fn done(reason: &str) -> Self {
        Self::new(Result3::Done, reason)
    }

    pub(crate) fn skipped(reason: &str) -> Self {
        Self::new(Result3::Skipped, reason)
    }

    pub(crate) fn failed(reason: &str) -> Self {
        Self::new(Result3::Failed, reason)
    }

    /// 답이 끝나 그 구역을 마치지 못했다.
    pub(crate) fn ended() -> Self {
        Self::failed(ANSWERS_ENDED)
    }

    fn new(result: Result3, reason: &str) -> Self {
        Self {
            result,
            reason: reason.to_string(),
        }
    }

    /// 답이 끝나서 실패했는가(남은 구역은 돌지 않는다).
    pub(crate) fn input_ended(&self) -> bool {
        self.result == Result3::Failed && self.reason == ANSWERS_ENDED
    }
}

/// 끝 요약을 쓴다: 제목 줄, 표, `doctor` 안내.
pub(crate) fn write_summary(prompter: &mut Prompter<'_>, rows: &[(SetupSection, Outcome)]) {
    prompter.say("");
    prompter.say(SETUP_SUMMARY_TITLE);
    prompter.say("| 구역 | 결과 | 까닭 |");
    prompter.say("|---|---|---|");
    for (section, outcome) in rows {
        let result = match outcome.result {
            Result3::Done => SETUP_DONE,
            Result3::Skipped => SETUP_SKIPPED,
            Result3::Failed => SETUP_FAILED,
        };
        let reason = if outcome.reason.is_empty() {
            "-"
        } else {
            outcome.reason.as_str()
        };
        prompter.say(&format!(
            "| {} | {result} | {} |",
            section.as_str(),
            reason.replace('|', "/")
        ));
    }
    prompter.say("");
    prompter.say(SETUP_DOCTOR_HINT);
}

/// 실패한 구역이 있는가.
pub(crate) fn any_failed(rows: &[(SetupSection, Outcome)]) -> bool {
    rows.iter()
        .any(|(_, outcome)| outcome.result == Result3::Failed)
}
