//! (48차) 진단: 시연에서 받아 둔 키(확인 대기)의 수를 키 줄에 더한다. 값은 싣지 않는다(칸 이름도 싣지 않는다).

use std::path::Path;

use super::Check;

/// 확인 대기 키가 있으면 `keys`의 글 뒤에 "시연에서 받아 둔 키 n개 — 다음 실행에서 확인합니다"를 더한다.
pub(super) fn with_line(mut keys: Check, home: &Path) -> Check {
    if let Some(line) = crate::unverified::doctor_line(home) {
        keys.what = format!("{} · {line}", keys.what);
    }
    keys
}
