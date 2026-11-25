//! 증권 키 구역: `/keys` 화면의 칸과 같게(`pi_secrets::KeyKind::Broker`의 칸 차례) 한 줄씩 받아 도는 서버의
//! [`crate::SETUP_KEYS_PATH`]로 등록한다(등록 전 확인 · 시도 제한 · 원자 쓰기는 서버가 한다).
//! 첫 칸이 빈 줄이면 건너뛴다. 거절되면 까닭을 보이고 모든 칸을 다시 묻는다.

use pi_secrets::KeyKind;
use zeroize::Zeroizing;

use super::prompt::Prompter;
use super::server::{Link, Reply, gives_up};
use super::summary::Outcome;
use crate::setup::{KEY_BROKER, SETUP_KEYS_PATH};

/// 증권 키 구역을 돈다.
pub(crate) async fn broker(link: &Link, prompter: &mut Prompter<'_>) -> Outcome {
    prompter.title("증권 키 (토스증권 — 모의계좌라 이 칸들만)");
    prompter.say("글자는 보이지 않게 읽고, 저장하기 전에 서버가 증권사에 확인합니다. 첫 칸이 빈 줄이면 건너뜁니다");
    let fields = KeyKind::Broker.fields();
    loop {
        let mut values: Vec<(&str, Zeroizing<String>)> = Vec::new();
        for (at, field) in fields.iter().enumerate() {
            let Some(value) = prompter.secret(&format!("{field}:")) else {
                return Outcome::ended();
            };
            if at == 0 && value.is_empty() {
                return Outcome::skipped("넣지 않음");
            }
            values.push((field, value));
        }
        let body = key_body(KEY_BROKER, &values);
        drop(values);
        match link.post(SETUP_KEYS_PATH, body).await {
            Reply::Accepted => {
                prompter.say("증권 키를 등록했습니다");
                return Outcome::done("서버가 확인하고 등록함");
            }
            Reply::Refused(refusal) => {
                prompter.say(&refusal.to_string());
                if gives_up(refusal) {
                    return Outcome::failed(&refusal.to_string());
                }
                prompter.say("다시 넣으세요");
            }
            Reply::Broken => return Outcome::failed(super::BROKEN),
        }
    }
}

/// 키 등록 본문 `{"item": .., "fields": {칸: 값, ..}}`. 값은 중간 글자 없이 바로 본문에 쓴다
/// (늘리며 옮기다 옛 자리에 조각이 남지 않게 넉넉히 잡아 둔다).
pub(crate) fn key_body(item: &str, values: &[(&str, Zeroizing<String>)]) -> Vec<u8> {
    let room: usize = values
        .iter()
        .map(|(name, value)| name.len() + value.len() * 6 + 8)
        .sum();
    let mut body = Vec::with_capacity(room + item.len() + 64);
    body.extend_from_slice(b"{\"item\":");
    let _ = serde_json::to_writer(&mut body, item);
    body.extend_from_slice(b",\"fields\":{");
    for (at, (name, value)) in values.iter().enumerate() {
        if at > 0 {
            body.push(b',');
        }
        let _ = serde_json::to_writer(&mut body, name);
        body.push(b':');
        let _ = serde_json::to_writer(&mut body, value.as_str());
    }
    body.extend_from_slice(b"}}");
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 키_본문은_화면과_같은_모양이다() {
        let values = [
            ("client_id", Zeroizing::new("a\"b".to_string())),
            ("client_secret", Zeroizing::new("c".to_string())),
        ];
        let body = key_body("broker", &values);
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"item": "broker", "fields": {"client_id": "a\"b", "client_secret": "c"}})
        );
    }
}
