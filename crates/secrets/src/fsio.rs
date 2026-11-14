//! 보관 폴더와 키 파일의 파일 작업: 폴더 정하기 · 권한 좁히기 · 읽기 · 원자 쓰기.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use zeroize::{Zeroize, Zeroizing};

use crate::{DEFAULT_DIR_NAME, SecretsError};

/// 보관 폴더 경로를 정한다. 환경을 직접 읽지 않고 받은 값으로만 정한다(테스트가 환경을 바꾸지 않게).
///
/// - `env_override`: [`HOME_ENV`](crate::HOME_ENV)의 값. 있으면 그 경로가 보관 폴더다. 절대 경로여야
///   한다. 빈 글자이거나 상대 경로면 홈 폴더가 멀쩡해도 [`SecretsError::HomeNotFound`]로 거부한다
///   (잘못 넣은 설정을 말없이 기본 폴더로 바꾸지 않는다).
/// - `home_dir`: 사용자 홈 폴더. `env_override`가 없을 때(`None`)만 쓴다: `home_dir/`[`DEFAULT_DIR_NAME`].
///   빈 글자이거나 상대 경로면 [`SecretsError::HomeNotFound`].
/// - 둘 다 없으면 [`SecretsError::HomeNotFound`].
///
/// 상대 경로를 받지 않는 까닭: 프로그램을 띄운 현재 폴더 아래에 키 파일을 쓰지 않기 위해서다.
pub fn resolve_dir(
    env_override: Option<&OsStr>,
    home_dir: Option<&Path>,
) -> Result<PathBuf, SecretsError> {
    let dir = match env_override {
        Some(dir) => PathBuf::from(dir),
        None => {
            let home = home_dir.ok_or(SecretsError::HomeNotFound)?;
            require_absolute(home)?;
            home.join(DEFAULT_DIR_NAME)
        }
    };
    require_absolute(&dir)?;
    Ok(dir)
}

/// 보관 폴더로 쓸 수 있는 경로인가: 절대 경로만 받는다(빈 글자 · 상대 경로는 [`SecretsError::HomeNotFound`]).
pub(crate) fn require_absolute(dir: &Path) -> Result<(), SecretsError> {
    if dir.is_absolute() {
        Ok(())
    } else {
        Err(SecretsError::HomeNotFound)
    }
}

/// 보관 폴더를 준비한다. 없으면 0700으로 만들고, 있는데 남이 볼 수 있으면 0700으로 좁힌다(Unix).
/// 그 자리에 폴더가 아닌 것이 있으면 건드리지 않고 오류를 낸다.
pub(crate) fn prepare_dir(dir: &Path) -> Result<(), SecretsError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .map_err(SecretsError::io("폴더 만들기"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::metadata(dir).map_err(SecretsError::io("폴더 보기"))?;
        if meta.permissions().mode() & 0o077 != 0 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                .map_err(SecretsError::io("폴더 권한 좁히기"))?;
        }
    }
    Ok(())
}

/// 연 파일의 권한이 0600보다 넓으면 0600으로 좁힌다(Unix). 다른 곳에서는 아무것도 하지 않는다.
fn narrow_file(file: &fs::File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

/// 키 파일을 읽는다. 없으면 `Ok(None)`.
///
/// 내용을 읽기 **전에** 권한을 0600으로 좁힌다. UTF-8이 아니면 [`SecretsError::Corrupt`]
/// (`file`은 오류에 실을 파일 이름). 내용은 오류에 싣지 않는다.
pub(crate) fn read_private(
    path: &Path,
    file: &'static str,
) -> Result<Option<Zeroizing<String>>, SecretsError> {
    let mut handle = match fs::File::open(path) {
        Ok(handle) => handle,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(SecretsError::io("열기")(e)),
    };
    narrow_file(&handle).map_err(SecretsError::io("권한 좁히기"))?;
    let size = handle
        .metadata()
        .ok()
        .and_then(|meta| usize::try_from(meta.len()).ok())
        .unwrap_or(0);
    let mut bytes = Zeroizing::new(Vec::with_capacity(size.saturating_add(1)));
    handle
        .read_to_end(&mut bytes)
        .map_err(SecretsError::io("읽기"))?;
    match String::from_utf8(std::mem::take(&mut *bytes)) {
        Ok(text) => Ok(Some(Zeroizing::new(text))),
        Err(e) => {
            e.into_bytes().zeroize();
            Err(SecretsError::Corrupt { file })
        }
    }
}

/// `dir/name`을 `contents`로 바꿔 끼운다(원자 쓰기).
///
/// 같은 폴더에 처음부터 0600인 임시 파일을 만들어 다 쓰고 디스크에 내린 뒤 이름을 바꾼다.
/// 보는 쪽에는 옛 파일이나 새 파일만 보인다. 실패하면 임시 파일은 지워진다.
pub(crate) fn write_atomic(dir: &Path, name: &str, contents: &[u8]) -> Result<(), SecretsError> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(".tmp-").suffix(".part");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o600));
    }
    let mut tmp = builder
        .tempfile_in(dir)
        .map_err(SecretsError::io("임시 파일 만들기"))?;
    tmp.write_all(contents).map_err(SecretsError::io("쓰기"))?;
    tmp.as_file()
        .sync_all()
        .map_err(SecretsError::io("디스크에 내리기"))?;
    tmp.persist(dir.join(name))
        .map_err(|e| SecretsError::io("이름 바꾸기")(e.error))?;
    // 이름 바꾼 것을 디스크에 내린다. 폴더를 열 수 없는 곳(Windows 등)에서는 넘어간다.
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 원자_쓰기는_내용을_바꾸고_임시_파일을_남기지_않는다() {
        let tmp = tempfile::tempdir().unwrap();
        write_atomic(tmp.path(), "키-파일", "첫 내용".as_bytes()).unwrap();
        write_atomic(tmp.path(), "키-파일", "둘째 내용".as_bytes()).unwrap();
        let path = tmp.path().join("키-파일");
        assert_eq!(
            read_private(&path, "키-파일").unwrap().unwrap().as_str(),
            "둘째 내용"
        );
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn 없는_파일은_없다고_하고_utf8이_아니면_깨진_것이다() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("키-파일");
        assert!(read_private(&path, "키-파일").unwrap().is_none());
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(
            read_private(&path, "키-파일"),
            Err(SecretsError::Corrupt { file: "키-파일" })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn 폴더가_없어진_곳에_쓰면_값_없는_오류다() {
        let tmp = tempfile::tempdir().unwrap();
        let gone = tmp.path().join("없는-폴더");
        assert!(matches!(
            write_atomic(&gone, "키-파일", b"x"),
            Err(SecretsError::Io { .. })
        ));
    }
}
