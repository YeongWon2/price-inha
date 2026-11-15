//! 답 글 안의 JSON 행동(결정 12번).
//!
//! 도구 호출은 공급자의 네이티브 도구 호출이 아니라 답 글 안의 JSON 객체다.
//! API 키 공급자와 CLI 공급자가 같은 길로 간다.
//!
//! 형식: `{"action": "<도구 이름>", "args": { … }}`
//!
//! - 답 글에서 균형 잡힌 JSON 객체를 앞에서부터 찾는다. 앞뒤 산문 · 코드펜스가 있어도 된다.
//! - `"action"` 칸이 있는 객체만 행동이다. 그 밖의 객체(점수 JSON 등)는 행동이 아니다.
//! - `args`가 없으면 빈 객체(`{}`)로 본다.
//! - 다음은 깨진 행동이다: `"action"`이 든 `{`가 균형이 맞지 않거나 JSON이 아니다,
//!   `action`이 빈 글이거나 글이 아니다, `args`가 객체가 아니다.

/// 행동 하나(도구 호출 하나).
#[derive(Debug, Clone, PartialEq)]
pub struct ActionCall {
    /// 도구 이름.
    pub name: String,
    /// 인자. 언제나 JSON 객체다.
    pub args: serde_json::Value,
}

/// 답 글을 해석한 것.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// 행동이 없는 최종 답. 앞뒤 공백을 뗀 글이다.
    Final(String),
    /// 행동이 하나 이상 있다. 나온 순서대로다.
    Actions(Vec<ActionCall>),
    /// 깨진 행동이 있다. 깨진 것이 하나라도 있으면 다른 행동도 돌리지 않는다.
    Malformed,
    /// 글이 비었거나 공백뿐이다.
    Empty,
}

/// 답 글을 해석한다.
pub fn parse_reply(text: &str) -> Reply {
    let _ = text;
    todo!()
}

/// 답 글에서 `key` 칸이 있는 첫 번째 균형 잡힌 JSON 객체를 찾는다(앞뒤 산문 · 코드펜스 허용).
/// 점수 · 판단 · 주문 안 턴의 답을 읽을 때 쓴다. 없으면 `None`.
pub fn find_object_with(
    text: &str,
    key: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let _ = (text, key);
    todo!()
}
