//! 옛 구독 토큰 파일([`LEGACY_AUTH_FILE`](crate::LEGACY_AUTH_FILE))이 남아 있는지만 본다 (4차 2번).

use std::path::Path;

/// 보관 폴더 `dir`에 [`LEGACY_AUTH_FILE`](crate::LEGACY_AUTH_FILE) 이름의 항목이 있는지만 알려 준다.
///
/// - 항목의 메타데이터만 본다(`symlink_metadata`, 링크를 따라가지 않는다). 열지도, 읽지도,
///   고치지도, 지우지도 않는다. 권한 · 크기 · 수정 시각 · 내용이 그대로다.
/// - 읽을 수 없는 권한(0o000)의 파일, 심볼릭 링크(가리키는 곳이 없어도), 폴더여도 "있음"(`true`)이다.
/// - 보관 폴더가 없거나 그 이름의 항목이 없으면 "없음"(`false`)이다.
/// - 오류를 돌려주지 않는다. 볼 수 없으면 "없음"이다.
pub fn legacy_auth_file_present(dir: &Path) -> bool {
    let _ = dir;
    todo!("4차 2번: 옛 auth.json이 있는지만 본다")
}
