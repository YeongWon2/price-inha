//! 묶음 검증: 판 번호 · 경로 탈출 · 빠진 것 · 더 있는 것 · 내용 · SQLite 온전함.

use crate::common::*;
use pi_backup::{BackupError, BundleProblem, SNAPSHOT_FILE};

fn problem(bundle: &pi_backup::Bundle) -> BundleProblem {
    match pi_backup::verify(bundle) {
        Err(BackupError::InvalidBundle(found)) => found,
        other => panic!("검증이 거부해야 한다: {other:?}"),
    }
}

// 검증: V51, D7
#[test]
fn 온전한_묶음은_통과한다() {
    pi_backup::verify(&good_bundle()).unwrap();
}

// 검증: V51
#[test]
fn 상대_경로_한_조각이_아니면_경로_탈출로_거부한다() {
    let snap = snapshot_bytes();
    for path in [
        "../escape.txt",
        "/abs.txt",
        "dir/inner.txt",
        "..",
        ".",
        "",
        "a\\b.txt",
    ] {
        let bundle = bundle_of(vec![file(SNAPSHOT_FILE, &snap), file(path, b"x")]);

        assert_eq!(problem(&bundle), BundleProblem::UnsafePath, "{path:?}");
    }
}

// 검증: V51
#[test]
fn 판_번호가_다르면_거부한다() {
    let mut bundle = good_bundle();
    bundle.manifest.version = 2;

    assert_eq!(problem(&bundle), BundleProblem::UnsupportedVersion(2));
}

// 검증: V51
#[test]
fn manifest가_말한_파일이_없으면_거부한다() {
    let mut bundle = good_bundle();
    let (listed, _) = file("extra.txt", b"x");
    bundle.manifest.files.push(listed);

    assert_eq!(problem(&bundle), BundleProblem::MissingFile);
}

// 검증: V51
#[test]
fn manifest에_없는_파일이_있으면_거부한다() {
    let mut bundle = good_bundle();
    let (_, held) = file("extra.txt", b"x");
    bundle.files.push(held);

    assert_eq!(problem(&bundle), BundleProblem::UnlistedFile);
}

// 검증: V51
#[test]
fn 내용이나_크기가_manifest와_다르면_거부한다() {
    let mut changed = good_bundle();
    let last = changed.files[0].bytes.len() - 1;
    changed.files[0].bytes[last] ^= 0xff;
    assert_eq!(problem(&changed), BundleProblem::HashMismatch);

    let mut sized = good_bundle();
    sized.manifest.files[0].size += 1;
    assert_eq!(problem(&sized), BundleProblem::HashMismatch);
}

// 검증: V51
#[test]
fn 스냅샷이_없으면_거부한다() {
    let bundle = bundle_of(vec![file("other.txt", b"x")]);

    assert_eq!(problem(&bundle), BundleProblem::MissingSnapshot);
}

// 검증: V51
#[test]
fn 스냅샷이_온전한_저장소_파일이_아니면_거부한다() {
    let bundle = bundle_of(vec![file(SNAPSHOT_FILE, b"this is not a database at all")]);

    assert_eq!(problem(&bundle), BundleProblem::CorruptDatabase);
}

// 검증: D7
#[test]
fn 봉한_묶음을_풀면_그대로다() {
    let env_identity = age::x25519::Identity::generate();
    let recipient = env_identity.to_public().to_string();
    let bundle = good_bundle();

    let sealed = pi_backup::seal(&bundle, &recipient).unwrap();

    let identity = pi_secrets::SecretString::new(age::secrecy::ExposeSecret::expose_secret(
        &env_identity.to_string(),
    ));
    assert_eq!(pi_backup::open(&sealed, &identity).unwrap(), bundle);
}

// 검증: D7
#[test]
fn 잘못된_공개키나_identity는_거부한다() {
    let err = pi_backup::seal(&good_bundle(), "not-an-age-recipient").unwrap_err();
    assert!(matches!(err, BackupError::Encrypt), "{err:?}");

    let recipient = age::x25519::Identity::generate().to_public().to_string();
    let sealed = pi_backup::seal(&good_bundle(), &recipient).unwrap();
    let bad = pi_secrets::SecretString::new("not-an-age-identity");
    let err = pi_backup::open(&sealed, &bad).unwrap_err();
    assert!(matches!(err, BackupError::Decrypt), "{err:?}");
}
