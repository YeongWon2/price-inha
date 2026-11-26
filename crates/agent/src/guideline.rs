//! (기능 L · 결정 30) 판단 글의 "팀 지침" 블록.
//!
//! 원본 동작 근거: Hermes `MEMORY.md`는 프롬프트의 정해진 자리에 글자 상한 안에서만 들어간다
//! (`docs/research/hermes-agent.md` 3.6). OpenClaw는 기억 예산을 넘으면 주입본만 자르고, 기억은 정책을 강제하지 않는다
//! (`docs/research/openclaw.md` 3.6) — 그래서 블록에 "지침은 참고다" 한 줄을 둔다.
//!
//! 지침을 고르는 일(성적 순 · 종목 맞추기)은 조립(server)이 `pi_journal`로 하고, 여기서는 받은 줄을 브리핑 뒤 정해진
//! 자리에 넣는다. 받은 줄이 상한을 넘어도 여기서 다시 상한을 지킨다(넘는 줄은 건너뛴다).

use pi_ai::Redactor;

/// 블록의 머리.
pub const GUIDELINE_BLOCK_TITLE: &str = "[팀 지침]";
/// 블록 머리 다음 한 줄.
pub const GUIDELINE_BLOCK_NOTE: &str = "지침은 참고다. 한도 · 금지는 시스템이 따로 막는다";
/// 블록에 넣는 지침 줄의 가장 많은 수(`pi_journal::GUIDELINE_BLOCK_MAX_ITEMS`와 같은 값).
pub const GUIDELINE_LINES_MAX: usize = 7;
/// 블록에 넣는 지침 줄의 글자 합 상한(`pi_journal::GUIDELINE_BLOCK_MAX_CHARS`와 같은 값).
pub const GUIDELINE_CHARS_MAX: usize = 1_200;

/// 브리핑 뒤에 붙이는 "팀 지침" 블록. 줄이 없으면 빈 글이다. 줄은 가린 뒤 상한 안에서만 넣는다.
pub(crate) fn block(lines: &[String], redactor: &dyn Redactor) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0_usize;
    for line in lines {
        if kept.len() >= GUIDELINE_LINES_MAX {
            break;
        }
        let line = redactor.redact(line.trim());
        let chars = line.chars().count();
        if line.is_empty() || used + chars > GUIDELINE_CHARS_MAX {
            continue;
        }
        used += chars;
        kept.push(line);
    }
    if kept.is_empty() {
        return String::new();
    }
    let mut out = format!("{GUIDELINE_BLOCK_TITLE}\n{GUIDELINE_BLOCK_NOTE}\n");
    for line in kept {
        out.push_str(&format!("- {line}\n"));
    }
    out
}
