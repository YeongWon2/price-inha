//! (6차) 보관 폴더의 비밀이 아닌 설정 파일(하루 AI 상한 · 담당 모델 · 팀 지침 · 알림 설정 · 규칙 설정 …)을 백업 묶음에
//! 담는다. 원본에 없음(원본 백업은 스냅샷 하나) — 되살린 뒤 설정을 다시 하지 않게 한다.
//!
//! 묶음 안 이름은 파일 이름 그대로다(한 조각). 되살리면 대상 폴더 바로 아래(제자리)로 돌아간다. 다음 이름은 줘도 담지
//! 않는다: 키 파일([`pi_secrets::BACKUP_EXCLUDED_FILES`]) · 저장소 스냅샷 이름 · 보관 파일 앞붙이 · 점으로 시작하는
//! 이름 · 폴더 나눔이 든 이름. 심볼릭 링크 · 폴더는 따라가지 않는다. 담은 파일도 등록된 비밀 값 검사를 거친다(E39).

use std::path::Path;

use crate::{ARCHIVE_BUNDLE_PREFIX, BackupError, BundleFile, SNAPSHOT_FILE};

/// 담아도 되는 이름인가.
fn allowed(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && name != SNAPSHOT_FILE
        && !name.starts_with(ARCHIVE_BUNDLE_PREFIX)
        && !pi_secrets::BACKUP_EXCLUDED_FILES.contains(&name)
}

/// `home` 바로 아래의 설정 파일들(준 순서, 같은 이름은 한 번). 없는 파일 · 담을 수 없는 이름은 건너뛴다.
pub(crate) fn bundle_files(home: &Path, names: &[String]) -> Result<Vec<BundleFile>, BackupError> {
    let mut files: Vec<BundleFile> = Vec::new();
    for name in names {
        if !allowed(name) || files.iter().any(|f| &f.path == name) {
            continue;
        }
        let path = home.join(name);
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(BackupError::io(path, e)),
        };
        if !meta.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|e| BackupError::io(&path, e))?;
        files.push(BundleFile {
            path: name.clone(),
            bytes,
        });
    }
    Ok(files)
}
