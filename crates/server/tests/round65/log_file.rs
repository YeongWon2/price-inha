//! 기록 파일 돌려 쓰기(지시서 9차 보강 3 · 4 "파일 돌려 쓰기(크기 상한)"): `run --log-file`의 기록이 상한을 넘으면
//! 지금 파일을 `<이름>.1`로 옮기고(앞의 것은 `.2` …) 새 파일에 이어 쓴다. 남기는 옛 파일 수를 넘는 것은 지운다.
//! 줄은 가운데에서 자르지 않는다(쓰기 한 번 = 한 줄). 유닉스에서는 새 파일도 0600이다.

use std::io::Write;

use price_inha::{LOG_FILE_KEEP, LOG_FILE_MAX_BYTES, RotatingLogFile};

fn rotated(path: &std::path::Path, n: usize) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{n}"));
    name.into()
}

// 검증: D1 N8
#[test]
fn 상한을_넘으면_옛_파일로_돌리고_정한_수만_남긴다() {
    assert!(LOG_FILE_MAX_BYTES >= 1024 * 1024, "기본 상한은 넉넉하다");
    assert!(LOG_FILE_KEEP >= 1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("logs").join("server.log");
    let mut file = RotatingLogFile::open(&path, 100, 2).expect("폴더가 없어도 만든다");
    let line = |n: usize| format!("기록 줄 {n:03} {}\n", "x".repeat(20));
    for n in 0..20 {
        file.write_all(line(n).as_bytes()).unwrap();
    }
    file.flush().unwrap();
    let now = std::fs::read_to_string(&path).unwrap();
    let first = std::fs::read_to_string(rotated(&path, 1)).unwrap();
    let second = std::fs::read_to_string(rotated(&path, 2)).unwrap();
    assert!(!rotated(&path, 3).exists(), "옛 파일은 둘만 남긴다");
    for text in [&now, &first, &second] {
        assert!(text.len() as u64 <= 100, "파일마다 상한 안: {}", text.len());
        assert!(
            text.lines().all(|l| l.starts_with("기록 줄 ")),
            "줄을 자르지 않는다: {text}"
        );
    }
    assert!(
        now.contains("기록 줄 019"),
        "마지막 줄은 지금 파일에: {now}"
    );
    assert!(!now.contains("기록 줄 000"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in [path.clone(), rotated(&path, 1)] {
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{}", p.display());
        }
    }
}

// 검증: D1
#[test]
fn 다시_열면_있던_파일에_이어_쓰고_넘쳐_있으면_먼저_돌린다() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.log");
    std::fs::write(&path, "앞서 쓴 줄\n").unwrap();
    {
        let mut file = RotatingLogFile::open(&path, 1000, 1).unwrap();
        file.write_all("이어 쓴 줄\n".as_bytes()).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "앞서 쓴 줄\n이어 쓴 줄\n"
    );
    std::fs::write(&path, "y".repeat(2000)).unwrap();
    let mut file = RotatingLogFile::open(&path, 1000, 1).unwrap();
    file.write_all("새 줄\n".as_bytes()).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "새 줄\n");
    assert_eq!(
        std::fs::read_to_string(rotated(&path, 1)).unwrap().len(),
        2000
    );
}
