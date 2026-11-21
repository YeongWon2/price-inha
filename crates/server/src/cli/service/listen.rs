//! 등록된 정의가 서버를 어디에 여는지 읽는다(10차 10a 0번: `access-key`가 도는 서버를 찾을 때 쓴다).

use std::net::IpAddr;
use std::path::Path;

use super::{Service, same_path};

/// 등록된 정의에 실린 여는 주소. 정의에 없는 칸은 `None`(서버의 기본값을 쓴다는 뜻)이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct InstalledListen {
    /// `--bind <IP>`.
    pub(crate) bind: Option<IpAddr>,
    /// `--port <수>`.
    pub(crate) port: Option<u16>,
}

/// 정의 글(plist · unit · 예약 작업 XML)에서 `--bind` · `--port`의 값을 읽는다. 꼬리표(`<…>`)와 따옴표를
/// 걷어 낸 낱말들에서 인자 이름 바로 다음 낱말을 값으로 본다.
fn listen_of(contents: &str) -> InstalledListen {
    let mut plain = String::with_capacity(contents.len());
    let mut in_tag = false;
    for ch in contents.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                plain.push(' ');
            }
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }
    let words: Vec<&str> = plain
        .split_whitespace()
        .map(|word| word.trim_matches(|c| c == '"' || c == '\''))
        .collect();
    let value_of = |name: &str| {
        words
            .iter()
            .position(|word| *word == name)
            .and_then(|at| words.get(at + 1).copied())
    };
    InstalledListen {
        bind: value_of("--bind").and_then(|text| text.parse().ok()),
        port: value_of("--port").and_then(|text| text.parse().ok()),
    }
}

impl Service<'_> {
    /// 등록된 정의가 보관 폴더 `home`을 가리키면 그 정의에 실린 여는 주소. 정의가 없거나 다른 홈을
    /// 가리키거나 읽을 수 없으면 `None`(다른 홈의 정의는 쓰지 않는다).
    pub(crate) fn installed_listen(&self, home: &Path) -> Option<InstalledListen> {
        let installed = self.installed_home().ok().flatten()?;
        if !same_path(&installed, home) {
            return None;
        }
        let contents = std::fs::read_to_string(self.definition_path()).ok()?;
        Some(listen_of(&contents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 정의_글에서_여는_주소를_읽는다() {
        let plist = "<array><string>run</string><string>--home</string><string>/h</string>\
<string>--bind</string><string>192.168.0.7</string><string>--port</string><string>9000</string></array>";
        let found = listen_of(plist);
        assert_eq!(found.port, Some(9000));
        assert_eq!(found.bind, "192.168.0.7".parse().ok());
        let unit = "ExecStart=\"/opt/pi\" run --home \"/h\" --port 8123\n";
        assert_eq!(listen_of(unit).port, Some(8123));
        assert_eq!(listen_of(unit).bind, None);
        assert_eq!(listen_of("run --home /h"), InstalledListen::default());
    }
}
