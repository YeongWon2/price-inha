//! (41차 3번) 메신저 글 속 `@` 지목 읽기: 글 처음이거나 공백 뒤의 `@` 다음이 명단([`crate::OFFICE_ROSTER`])의 표시
//! 이름이나 담당 id로 시작하면 그 담당이다(가장 긴 이름 — 뒤에 조사가 붙어도 된다, id는 대소문자를 가리지 않는다).
//! 읽은 지목은 사무실 팀 방의 대괄호 꼴(`pi_office::mention_tag`)로 바꿔 `post_team`과 같은 길로 넘긴다.

use pi_office::{Agent, Member, mention_tag, mentioned_names, parse_mentions};

/// 읽은 글.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mentioned {
    /// 지목하려 한 자리(`@` 뒤에 글자)가 하나라도 있는가.
    pub(super) any: bool,
    /// 명단과 맞은 담당(글에 처음 나온 차례, 겹치지 않음).
    pub(super) agents: Vec<Agent>,
    /// 지목을 대괄호 꼴로 바꾼 글.
    pub(super) text: String,
}

/// 명단(표시 이름).
fn roster() -> Vec<Member> {
    crate::OFFICE_ROSTER
        .iter()
        .filter_map(|(id, name)| {
            Agent::from_id(id).map(|agent| Member {
                agent,
                name: (*name).to_owned(),
            })
        })
        .collect()
}

/// `rest`가 그 이름으로 시작하면 이름의 바이트 길이(id는 대소문자 없이).
fn starts_with(rest: &str, candidate: &str) -> Option<usize> {
    let head = rest.get(..candidate.len())?;
    head.eq_ignore_ascii_case(candidate)
        .then_some(candidate.len())
}

/// `@` 바로 뒤 글에서 가장 긴 명단 이름: (표시 이름, 쓴 바이트 수).
fn longest(rest: &str) -> Option<(&'static str, usize)> {
    crate::OFFICE_ROSTER
        .iter()
        .flat_map(|(id, name)| [(*name, *name), (*name, *id)])
        .filter_map(|(name, candidate)| starts_with(rest, candidate).map(|len| (name, len)))
        .max_by_key(|(_, len)| *len)
}

/// 글을 읽는다.
pub(super) fn read(text: &str) -> Mentioned {
    let mut out = String::with_capacity(text.len() + 8);
    let mut any = false;
    let mut after_space = true;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let tail = &rest[c.len_utf8()..];
        if c == '@' && after_space {
            any |= tail.chars().next().is_some_and(|n| !n.is_whitespace());
            if let Some((name, len)) = longest(tail) {
                out.push_str(&mention_tag(name));
                rest = &tail[len..];
                after_space = false;
                continue;
            }
        }
        out.push(c);
        after_space = c.is_whitespace();
        rest = tail;
    }
    let agents = parse_mentions(&out, &roster());
    Mentioned {
        any: any || !mentioned_names(&out).is_empty(),
        agents,
        text: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 이름_id_조사를_받고_대괄호_꼴로_바꾼다() {
        let read_one = read("@리스크는 손절을 어떻게 봐요?");
        assert_eq!(read_one.agents, vec![Agent::Risk]);
        assert_eq!(read_one.text, "@[리스크]는 손절을 어떻게 봐요?");
        let by_id = read("@Strategy 금리는?");
        assert_eq!(by_id.agents, vec![Agent::Strategy]);
        assert_eq!(by_id.text, "@[전략] 금리는?");
        let two = read("@리스크 @전략 둘 다");
        assert_eq!(two.agents, vec![Agent::Risk, Agent::Strategy]);
    }

    #[test]
    fn 없는_이름과_지목_없는_글을_가린다() {
        let none = read("@회계 이번 분기");
        assert!(none.any);
        assert!(none.agents.is_empty());
        let plain = read("오늘 시장 a@b 어때요");
        assert!(!plain.any);
        assert_eq!(plain.text, "오늘 시장 a@b 어때요");
    }
}
