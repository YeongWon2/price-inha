//! 진단(`price-inha doctor`). 원본 동작: Hermes 2.6(범주를 순서대로, 사람이 할 것을 번호로, 문제가 남으면
//! 종료 코드 1) · OpenClaw 3.6(`--json`). 자동으로 고치지 않는다. 바깥(증권사 · AI · GitHub)을 부르지 않는다.

use crate::Settings;
use crate::cli::service::Service;

/// 진단 항목. 이 순서대로 본다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckItem {
    /// 보관 폴더가 있고 0700, 비밀 파일 0600(유닉스).
    Home,
    /// SQLite가 열리고 `PRAGMA integrity_check`가 ok, 스키마 버전을 안다.
    Database,
    /// 키 칸마다 등록 여부(이름만).
    Keys,
    /// 여는 주소 · 포트를 쓸 수 있나(이미 도는 서버가 있으면 그 사실만).
    Address,
    /// 루프백이 아닌 주소에 여는데 접속 키가 없으면 문제.
    AccessKey,
    /// 서비스 등록 여부와 정의가 가리키는 홈, Linux면 linger.
    Service,
    /// 백업이 켜졌으면 마지막 백업 시각.
    Backup,
}

/// 항목 하나의 결과. 키 값 · 길이 · 앞뒤 글자를 싣지 않는다(N8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// 항목.
    pub item: CheckItem,
    /// 문제가 없나.
    pub ok: bool,
    /// 무엇을 봤나(사람이 읽는 글).
    pub what: String,
    /// 문제면 사람이 할 일.
    pub fix: Option<String>,
}

/// 진단 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    /// 본 순서대로의 항목들.
    pub checks: Vec<Check>,
}

impl DoctorReport {
    /// 그 항목의 결과.
    pub fn check(&self, item: CheckItem) -> Option<&Check> {
        let _ = item;
        todo!()
    }

    /// 문제(✗) 수.
    pub fn problems(&self) -> usize {
        todo!()
    }

    /// 종료 코드: 문제가 남으면 1, 아니면 0.
    pub fn exit_code(&self) -> u8 {
        todo!()
    }

    /// 항목마다 `✓ / ✗ 무엇 · 고치기` 한 줄, 끝에 사람이 할 것을 번호로.
    pub fn render(&self) -> String {
        todo!()
    }

    /// `--json`의 답.
    pub fn to_json(&self) -> serde_json::Value {
        todo!()
    }
}

/// `settings`의 보관 폴더 · 주소 · 포트와 `service`의 등록 상태를 진단한다. 아무것도 고치지 않는다.
pub fn doctor(settings: &Settings, service: &Service<'_>) -> DoctorReport {
    let _ = (settings, service);
    todo!()
}
