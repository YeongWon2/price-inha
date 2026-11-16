//! 논의 방의 발언 상태: 누가 말했고 누가 불렸는가(결정 14번, 원본 Hermes 계획기와 같은 규칙).

use std::collections::HashMap;

use crate::role::Role;

/// 논의 하나의 발언 상태.
#[derive(Debug, Default)]
pub(crate) struct Floor {
    /// 방에 남은 글(쓴 담당, 글), 쓴 순서.
    said: Vec<(Role, String)>,
    /// 담당이 마지막으로 말한(넘긴) 때의 글 수.
    spoke: HashMap<Role, usize>,
    /// 담당이 마지막으로 불린 글의 글 수(그 글까지 센 수).
    called: HashMap<Role, usize>,
    /// 담당이 마지막으로 읽은 때의 글 수.
    seen: HashMap<Role, usize>,
}

impl Floor {
    /// 방에 남은 글 수.
    pub(crate) fn count(&self) -> usize {
        self.said.len()
    }

    /// 방에 남은 글 전부.
    pub(crate) fn said(&self) -> &[(Role, String)] {
        &self.said
    }

    /// 담당이 아직 읽지 않은 글. 읽은 것으로 표시한다.
    pub(crate) fn unseen(&mut self, role: Role) -> Vec<(Role, String)> {
        let from = self.seen.get(&role).copied().unwrap_or(0);
        self.seen.insert(role, self.said.len());
        self.said.get(from..).unwrap_or(&[]).to_vec()
    }

    /// 처음 읽는 담당인가.
    pub(crate) fn is_new(&self, role: Role) -> bool {
        !self.seen.contains_key(&role)
    }

    /// 담당이 글을 남겼다. 부른 담당들은 다음 라운드의 응답자 후보가 된다.
    pub(crate) fn say(&mut self, role: Role, text: String, handoff: &[Role]) {
        self.said.push((role, text));
        let count = self.said.len();
        self.spoke.insert(role, count);
        self.seen.insert(role, count);
        for called in handoff {
            self.called.insert(*called, count);
        }
    }

    /// 담당이 넘겼다. 그 전의 부름은 답한 것으로 본다.
    pub(crate) fn pass(&mut self, role: Role) {
        self.spoke.insert(role, self.said.len());
    }

    /// 다음 라운드의 응답자: 불린 뒤 아직 말하지 않은 담당, 명단 순서.
    pub(crate) fn pending(&self, members: &[Role]) -> Vec<Role> {
        members
            .iter()
            .copied()
            .filter(|role| {
                let called = self.called.get(role).copied().unwrap_or(0);
                called > self.spoke.get(role).copied().unwrap_or(0)
            })
            .collect()
    }
}

/// 넘김인가: 비었거나 `pass` · `(pass)` · `(pass).`(대소문자 무시).
pub(crate) fn is_pass(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    matches!(text.as_str(), "" | "pass" | "(pass)" | "(pass).")
}

/// 글이 `@<id>`로 부른 다른 참가 담당. 명단 순서, 겹침 없음. 자기 · 명단 밖은 뺀다.
pub(crate) fn mentions(text: &str, speaker: Role, members: &[Role]) -> Vec<Role> {
    let named: Vec<Role> = text
        .split('@')
        .skip(1)
        .filter_map(|rest| {
            let id: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
                .collect();
            Role::from_id(&id)
        })
        .collect();
    members
        .iter()
        .copied()
        .filter(|role| *role != speaker && named.contains(role))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 넘김_표식을_가린다() {
        for text in ["", " pass ", "(Pass)", "(PASS)."] {
            assert!(is_pass(text), "{text}");
        }
        assert!(!is_pass("pass 하겠다"));
    }

    #[test]
    fn 다시_불리면_다시_응답자가_된다() {
        let members = [Role::Scout, Role::Strategy];
        let mut floor = Floor::default();
        floor.say(Role::Scout, "@strategy".into(), &[Role::Strategy]);
        assert_eq!(floor.pending(&members), [Role::Strategy]);
        floor.say(Role::Strategy, "@scout".into(), &[Role::Scout]);
        assert_eq!(floor.pending(&members), [Role::Scout]);
        floor.pass(Role::Scout);
        assert!(floor.pending(&members).is_empty());
        assert_eq!(
            mentions("@SCOUT, @risk @strategy", Role::Strategy, &members),
            [Role::Scout]
        );
    }
}
