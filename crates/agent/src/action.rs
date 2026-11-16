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
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Reply::Empty;
    }
    let mut actions = Vec::new();
    for found in scan(text) {
        match found {
            Found::Broken => return Reply::Malformed,
            Found::Object(map) if map.contains_key("action") => match to_action(&map) {
                Some(call) => actions.push(call),
                None => return Reply::Malformed,
            },
            Found::Object(_) => {}
        }
    }
    if actions.is_empty() {
        Reply::Final(trimmed.to_string())
    } else {
        Reply::Actions(actions)
    }
}

/// 답 글에서 `key` 칸이 있는 첫 번째 균형 잡힌 JSON 객체를 찾는다(앞뒤 산문 · 코드펜스 허용).
/// 점수 · 판단 · 주문 안 턴의 답을 읽을 때 쓴다. 없으면 `None`.
pub fn find_object_with(
    text: &str,
    key: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    scan(text).into_iter().find_map(|found| match found {
        Found::Object(map) if map.contains_key(key) => Some(map),
        _ => None,
    })
}

/// 글에서 찾은 객체 하나.
enum Found {
    /// 맨 위 JSON 객체.
    Object(serde_json::Map<String, serde_json::Value>),
    /// `"action"`이 든 `{`인데 JSON 객체가 아니다(균형이 안 맞거나 JSON이 아니다).
    Broken,
}

/// 여는 중괄호. 글자 그대로 쓰면 함수 크기 검사기가 짝을 잘못 센다.
const OPEN_BRACE: char = '\u{7b}';

/// 앞에서부터 맨 위 JSON 객체를 찾는다. 찾은 객체 안쪽은 다시 보지 않는다.
fn scan(text: &str) -> Vec<Found> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(offset) = text.get(pos..).and_then(|rest| rest.find(OPEN_BRACE)) {
        let start = pos + offset;
        let rest = text.get(start..).unwrap_or("");
        let mut stream = serde_json::Deserializer::from_str(rest).into_iter::<serde_json::Value>();
        match stream.next() {
            Some(Ok(serde_json::Value::Object(map))) => {
                out.push(Found::Object(map));
                pos = start + stream.byte_offset().max(1);
            }
            _ => {
                if is_broken_action(rest) {
                    out.push(Found::Broken);
                }
                pos = start + 1;
            }
        }
    }
    out
}

/// JSON으로 읽히지 않은 `{`가 행동을 쓰려던 것인가.
/// `{` 다음이 JSON 키(따옴표)로 시작하고, 그 객체 자리(균형이 맞으면 닫는 `}`까지,
/// 아니면 글 끝까지)에 `"action"`이 있으면 그렇다.
fn is_broken_action(rest: &str) -> bool {
    let after = rest.get(1..).unwrap_or("").trim_start();
    if !after.starts_with('"') {
        return false;
    }
    let segment = match balanced_end(rest) {
        Some(end) => rest.get(..=end).unwrap_or(rest),
        None => rest,
    };
    segment.contains("\"action\"")
}

/// `rest`의 첫 `{`와 짝이 맞는 `}`의 바이트 자리. JSON 글 안의 괄호는 세지 않는다.
fn balanced_end(rest: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in rest.char_indices() {
        if in_string {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// `"action"` 칸이 있는 객체를 행동으로 바꾼다. 모양이 틀리면 `None`(깨진 행동).
fn to_action(map: &serde_json::Map<String, serde_json::Value>) -> Option<ActionCall> {
    let name = map.get("action")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let args = match map.get("args") {
        None => serde_json::Value::Object(serde_json::Map::new()),
        Some(value @ serde_json::Value::Object(_)) => value.clone(),
        Some(_) => return None,
    };
    Some(ActionCall {
        name: name.to_string(),
        args,
    })
}
