//! pi-secrets 4차 작업 테스트. 기준: `docs/dev/modules/secrets.md`의 "4차 작업" 1 · 2번.
//!
//! 옛 구독 토큰 파일([`LEGACY_AUTH_FILE`])이 보관 폴더에 있는지만 알려 준다
//! ([`legacy_auth_file_present`]). 열지도, 읽지도, 고치지도, 지우지도 않는다.
//! 읽을 수 없는 권한의 파일 · 심볼릭 링크 · 폴더여도 "있음"이고, 보관 폴더가 없으면 "없음"이다.
//!
//! - 보관 폴더는 모두 임시 폴더다. 실제 홈 폴더와 프로세스 환경 변수를 건드리지 않는다.
//! - 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use pi_secrets::{ENV_FILE, LEGACY_AUTH_FILE, legacy_auth_file_present};
use tempfile::tempdir;

const OLD_ACCESS: &str = "시험용-옛-접근-토큰-0001";
const OLD_REFRESH: &str = "시험용-옛-갱신-토큰-0001";

/// 1 · 2차 보관소가 쓰던 모양의 `auth.json` 내용.
fn legacy_auth_json() -> String {
    format!(
        "{{\n  \"ai_oauth\": {{\n    \"access_token\": \"{OLD_ACCESS}\",\n    \
         \"refresh_token\": \"{OLD_REFRESH}\"\n  }}\n}}\n"
    )
}

/// 테스트가 정하는 지난 시각. 파일의 수정 시각을 여기로 맞춰 두고, 부른 뒤에도 그대로인지 본다.
fn fixed_past() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000)
}

/// 옛 파일을 쓰고 수정 시각을 [`fixed_past`]로 맞춘다.
fn write_legacy(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(fixed_past()).unwrap();
}

/// 폴더 안 항목 이름들(정렬). 부른 뒤에 새 항목(임시 파일 등)이 생기거나 사라지지 않았는지 본다.
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 링크를 따라가지 않는 메타데이터에서 크기 · 수정 시각 · (Unix) 권한을 뽑는다.
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    len: u64,
    modified: SystemTime,
    mode: Option<u32>,
    is_file: bool,
    is_dir: bool,
    is_symlink: bool,
}

fn snapshot(path: &Path) -> Snapshot {
    let meta = fs::symlink_metadata(path).unwrap();
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let mode = None;
    Snapshot {
        len: meta.len(),
        modified: meta.modified().unwrap(),
        mode,
        is_file: meta.file_type().is_file(),
        is_dir: meta.file_type().is_dir(),
        is_symlink: meta.file_type().is_symlink(),
    }
}

// ───────────── 1. 파일 이름 ─────────────

// 검증: D3, N8
#[test]
fn 옛_파일_이름은_auth_json이고_그_이름으로_있으면_있음이다() {
    assert_eq!(LEGACY_AUTH_FILE, "auth.json");
    assert_ne!(LEGACY_AUTH_FILE, ENV_FILE, "키 파일과 다른 이름이다");

    let tmp = tempdir().unwrap();
    write_legacy(&tmp.path().join("auth.json"), legacy_auth_json().as_bytes());
    assert!(legacy_auth_file_present(tmp.path()));
}

// ───────────── 2. 없음 ─────────────

// 검증: D3, V46
#[test]
fn 보관_폴더가_없으면_없음이고_폴더를_만들지_않는다() {
    let tmp = tempdir().unwrap();
    let missing = tmp.path().join("없는-보관-폴더");

    assert!(!legacy_auth_file_present(&missing));
    assert!(
        fs::symlink_metadata(&missing).is_err(),
        "있는지 보기만 한다. 보관 폴더를 만들지 않는다"
    );
    assert!(entries(tmp.path()).is_empty());
}

// 검증: D3, V46
#[test]
fn 보관_폴더에_그_이름의_항목이_없으면_없음이고_아무것도_만들지_않는다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path();
    // 다른 파일은 있다: 키 파일, 이름이 비슷한 파일들.
    fs::write(dir.join(ENV_FILE), "AI_API_KEY=시험용-에이아이-값-0001\n").unwrap();
    fs::write(dir.join("auth.json.bak"), legacy_auth_json()).unwrap();
    fs::write(dir.join("old-auth.json"), legacy_auth_json()).unwrap();
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub").join("auth.json"), legacy_auth_json()).unwrap();
    let before = entries(dir);

    assert!(
        !legacy_auth_file_present(dir),
        "이름이 정확히 같은 항목만 본다. 아래 폴더는 보지 않는다"
    );
    assert_eq!(entries(dir), before, "새 항목이 생기지 않는다");
    assert!(
        fs::symlink_metadata(dir.join(LEGACY_AUTH_FILE)).is_err(),
        "옛 파일을 만들지 않는다"
    );
}

// 검증: D3, V46
#[test]
fn 빈_보관_폴더면_없음이다() {
    let tmp = tempdir().unwrap();
    assert!(!legacy_auth_file_present(tmp.path()));
    assert!(entries(tmp.path()).is_empty());
}

// ───────────── 3. 있음, 그리고 그대로 둔다 ─────────────

// 검증: D3, N8, V46, E38
#[test]
fn 옛_파일이_있으면_있음이고_크기_수정시각_내용이_그대로다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path();
    let legacy = dir.join(LEGACY_AUTH_FILE);
    let content = legacy_auth_json();
    write_legacy(&legacy, content.as_bytes());
    let before = snapshot(&legacy);
    let names = entries(dir);

    assert!(legacy_auth_file_present(dir));
    assert!(legacy_auth_file_present(dir), "여러 번 불러도 같다");

    assert_eq!(
        snapshot(&legacy),
        before,
        "크기 · 수정 시각 · 권한이 그대로다"
    );
    assert_eq!(before.modified, fixed_past());
    assert!(before.is_file && !before.is_symlink);
    assert_eq!(
        fs::read_to_string(&legacy).unwrap(),
        content,
        "내용이 그대로다"
    );
    assert_eq!(entries(dir), names, "지우지도, 새 파일을 만들지도 않는다");
}

// 검증: D3, V46
#[test]
fn 빈_옛_파일도_있음이다() {
    let tmp = tempdir().unwrap();
    let legacy = tmp.path().join(LEGACY_AUTH_FILE);
    write_legacy(&legacy, b"");
    let before = snapshot(&legacy);

    assert!(legacy_auth_file_present(tmp.path()));
    assert_eq!(snapshot(&legacy), before);
    assert_eq!(fs::read(&legacy).unwrap(), Vec::<u8>::new());
}

// 검증: D3, V46
#[test]
fn 깨진_옛_파일도_있음이고_그대로다() {
    let tmp = tempdir().unwrap();
    let legacy = tmp.path().join(LEGACY_AUTH_FILE);
    let broken: &[u8] = &[0xff, 0xfe, b'{', 0x00, 0x80];
    write_legacy(&legacy, broken);
    let before = snapshot(&legacy);

    assert!(legacy_auth_file_present(tmp.path()));
    assert_eq!(snapshot(&legacy), before);
    assert_eq!(fs::read(&legacy).unwrap(), broken);
}

// 검증: D3, N8, V46
#[cfg(unix)]
#[test]
fn 넓은_권한의_옛_파일도_권한을_좁히지_않는다() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let legacy = tmp.path().join(LEGACY_AUTH_FILE);
    write_legacy(&legacy, legacy_auth_json().as_bytes());
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o644)).unwrap();
    let before = snapshot(&legacy);

    assert!(legacy_auth_file_present(tmp.path()));
    assert_eq!(
        snapshot(&legacy),
        before,
        "키 파일과 달리 옛 파일의 권한은 고치지 않는다(사용자의 파일이다)"
    );
    assert_eq!(before.mode, Some(0o644));
}

// 검증: D3, N8, V46, E38
#[cfg(unix)]
#[test]
fn 읽을_수_없는_권한의_옛_파일도_있음이고_권한_크기_수정시각_내용이_그대로다() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let legacy = tmp.path().join(LEGACY_AUTH_FILE);
    let content = legacy_auth_json();
    write_legacy(&legacy, content.as_bytes());
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o000)).unwrap();
    let before = snapshot(&legacy);

    let present = legacy_auth_file_present(tmp.path());
    let after = snapshot(&legacy);
    // 내용을 보려고 권한을 되돌린다(확인은 결과와 메타데이터를 먼저 잡아 둔 뒤에 한다).
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o600)).unwrap();

    assert!(present, "열지 않고 있는지만 보므로 읽을 수 없어도 있음이다");
    assert_eq!(after, before, "권한 0o000 · 크기 · 수정 시각이 그대로다");
    assert_eq!(before.mode, Some(0o000));
    assert_eq!(fs::read_to_string(&legacy).unwrap(), content);
}

// 검증: D3, V46
#[cfg(unix)]
#[test]
fn 가리키는_곳이_없는_심볼릭_링크도_있음이고_링크를_그대로_둔다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path();
    let legacy = dir.join(LEGACY_AUTH_FILE);
    let target = dir.join("없는-대상");
    std::os::unix::fs::symlink(&target, &legacy).unwrap();
    let link_before = fs::read_link(&legacy).unwrap();
    let names = entries(dir);

    assert!(
        legacy_auth_file_present(dir),
        "링크를 따라가지 않고 그 이름의 항목이 있는지만 본다"
    );
    let meta = fs::symlink_metadata(&legacy).unwrap();
    assert!(meta.file_type().is_symlink(), "링크가 그대로 있다");
    assert_eq!(fs::read_link(&legacy).unwrap(), link_before);
    assert!(
        fs::symlink_metadata(&target).is_err(),
        "링크가 가리키는 곳을 만들지 않는다"
    );
    assert_eq!(entries(dir), names);
}

// 검증: D3, N8, V46
#[cfg(unix)]
#[test]
fn 파일을_가리키는_심볼릭_링크도_있음이고_링크와_대상을_그대로_둔다() {
    let outer = tempdir().unwrap();
    let vault = tempdir().unwrap();
    let target = outer.path().join("다른-곳-auth.json");
    let content = legacy_auth_json();
    write_legacy(&target, content.as_bytes());
    let target_before = snapshot(&target);
    let legacy = vault.path().join(LEGACY_AUTH_FILE);
    std::os::unix::fs::symlink(&target, &legacy).unwrap();

    assert!(legacy_auth_file_present(vault.path()));
    assert!(snapshot(&legacy).is_symlink, "링크를 파일로 바꾸지 않는다");
    assert_eq!(fs::read_link(&legacy).unwrap(), target);
    assert_eq!(snapshot(&target), target_before, "대상 파일이 그대로다");
    assert_eq!(fs::read_to_string(&target).unwrap(), content);
}

// 검증: D3, V46
#[test]
fn 그_이름의_폴더도_있음이고_폴더와_안의_것을_그대로_둔다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path();
    let legacy = dir.join(LEGACY_AUTH_FILE);
    fs::create_dir(&legacy).unwrap();
    let inner = legacy.join("안의-파일");
    write_legacy(&inner, legacy_auth_json().as_bytes());
    let inner_before = snapshot(&inner);
    let inside = entries(&legacy);
    let names = entries(dir);

    assert!(legacy_auth_file_present(dir));
    let meta = fs::symlink_metadata(&legacy).unwrap();
    assert!(meta.file_type().is_dir(), "폴더가 그대로 있다");
    assert_eq!(entries(&legacy), inside, "폴더 안을 지우거나 더하지 않는다");
    assert_eq!(snapshot(&inner), inner_before);
    assert_eq!(fs::read_to_string(&inner).unwrap(), legacy_auth_json());
    assert_eq!(entries(dir), names);
}

// 검증: D3, N8, V46
#[cfg(unix)]
#[test]
fn 읽을_수_없는_권한의_폴더여도_있음이고_권한을_그대로_둔다() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let legacy = tmp.path().join(LEGACY_AUTH_FILE);
    fs::create_dir(&legacy).unwrap();
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o000)).unwrap();
    let before = snapshot(&legacy);

    let present = legacy_auth_file_present(tmp.path());
    let after = snapshot(&legacy);
    // 임시 폴더를 치울 수 있게 권한을 되돌린다.
    fs::set_permissions(&legacy, fs::Permissions::from_mode(0o700)).unwrap();

    assert!(present);
    assert_eq!(after, before, "폴더 권한 0o000이 그대로다");
    assert!(after.is_dir);
}
