//! 되살리기(V51): 빈 폴더에만, 쓰기 전에 검증, identity는 남기지 않음, 끝나면 팀 멈춤 · 승인 만료.

use crate::common::*;
use pi_backup::{BackupError, BundleProblem, SNAPSHOT_FILE, restore};
use pi_secrets::SecretString;
use pi_store::{ApprovalStatus, Store, TeamState, UnixMillis};

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);
const LATER: UnixMillis = UnixMillis(NOW.0 + 3_600_000);

/// 기다림 승인 하나를 담아 올리고 그 암호문과 승인 번호를 돌려준다.
async fn uploaded(env: &Env) -> (Vec<u8>, pi_store::ApprovalId) {
    let id = pending_approval(
        &env.store,
        "되살릴 때 만료될 승인",
        UnixMillis(NOW.0 - 1000),
    );
    let report = env.backup().run(NOW).await.unwrap();
    (env.fake.file(&report.remote_path).unwrap(), id)
}

fn assert_untouched(env: &Env, target: &std::path::Path) {
    assert!(!target.exists(), "대상을 만들지 않는다");
    assert!(!env.dir.path().join("escape.txt").exists());
}

// 검증: V51, D7
#[tokio::test]
async fn 되살리면_팀이_멈추고_기다리던_승인이_만료된다() {
    let env = Env::new().await;
    let (sealed, id) = uploaded(&env).await;
    let target = env.target("restored");
    std::fs::create_dir(&target).unwrap();

    let report = restore(&sealed, &env.identity_secret(), &target, LATER).unwrap();

    assert_eq!(report.outcome.expired_approvals, vec![id]);
    assert!(report.outcome.team_record.is_some());
    assert_eq!(report.manifest.files[0].path, SNAPSHOT_FILE);
    let back = Store::open(target.join(SNAPSHOT_FILE)).unwrap();
    assert_eq!(back.team_state().unwrap(), TeamState::FullStop);
    let approval = back.get_approval(id).unwrap();
    assert_eq!(approval.status, ApprovalStatus::Expired);
    // 원래 저장소는 그대로다.
    assert_eq!(env.store.team_state().unwrap(), TeamState::Resting);
}

// 검증: V51
#[tokio::test]
async fn 없는_폴더면_만들어_되살린다() {
    let env = Env::new().await;
    let (sealed, _) = uploaded(&env).await;
    let target = env.target("fresh");

    restore(&sealed, &env.identity_secret(), &target, LATER).unwrap();

    assert!(target.join(SNAPSHOT_FILE).is_file());
}

// 검증: V51
#[tokio::test]
async fn 비어_있지_않은_폴더에는_되살리지_않는다() {
    let env = Env::new().await;
    let (sealed, _) = uploaded(&env).await;
    let target = env.target("busy");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep.txt"), b"mine").unwrap();

    let err = restore(&sealed, &env.identity_secret(), &target, LATER).unwrap_err();

    assert!(matches!(err, BackupError::TargetNotEmpty(_)), "{err:?}");
    assert_eq!(names_in(&target), vec!["keep.txt".to_owned()]);
    assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"mine");
}

// 검증: V51
#[tokio::test]
async fn 대상이_파일이면_되살리지_않는다() {
    let env = Env::new().await;
    let (sealed, _) = uploaded(&env).await;
    let target = env.target("a-file");
    std::fs::write(&target, b"mine").unwrap();

    let err = restore(&sealed, &env.identity_secret(), &target, LATER).unwrap_err();

    assert!(matches!(err, BackupError::TargetNotEmpty(_)), "{err:?}");
    assert_eq!(std::fs::read(&target).unwrap(), b"mine");
}

// 검증: V51, D7
#[tokio::test]
async fn 맞지_않는_identity면_아무것도_쓰지_않는다() {
    let env = Env::new().await;
    let (sealed, _) = uploaded(&env).await;
    let target = env.target("never");
    let other = age::x25519::Identity::generate();
    let other = SecretString::new(age::secrecy::ExposeSecret::expose_secret(
        &other.to_string(),
    ));

    let err = restore(&sealed, &other, &target, LATER).unwrap_err();

    assert!(matches!(err, BackupError::Decrypt), "{err:?}");
    assert_untouched(&env, &target);
}

/// 손으로 만든 묶음을 되살려 본다. 검증에서 걸려야 하고 아무것도 쓰지 않아야 한다.
async fn rejected(bundle: pi_backup::Bundle, want: BundleProblem) {
    let env = Env::new().await;
    let sealed = env.seal(&bundle);
    let target = env.target("never");

    let err = restore(&sealed, &env.identity_secret(), &target, LATER).unwrap_err();

    assert!(
        matches!(&err, BackupError::InvalidBundle(found) if *found == want),
        "{err:?}"
    );
    assert_untouched(&env, &target);
}

// 검증: V51
#[tokio::test]
async fn 경로_탈출이_든_묶음은_쓰기_전에_거부한다() {
    let bundle = bundle_of(vec![
        file(SNAPSHOT_FILE, &snapshot_bytes()),
        file("../escape.txt", b"outside"),
    ]);
    rejected(bundle, BundleProblem::UnsafePath).await;
}

// 검증: V51
#[tokio::test]
async fn 빠진_파일이_있는_묶음은_쓰기_전에_거부한다() {
    let mut bundle = good_bundle();
    bundle.manifest.files.push(file("extra.txt", b"x").0);
    rejected(bundle, BundleProblem::MissingFile).await;
}

// 검증: V51
#[tokio::test]
async fn 내용이_바뀐_묶음은_쓰기_전에_거부한다() {
    let mut bundle = good_bundle();
    bundle.files[0].bytes[0] ^= 0xff;
    rejected(bundle, BundleProblem::HashMismatch).await;
}

// 검증: V51
#[tokio::test]
async fn 깨진_저장소_파일이_든_묶음은_쓰기_전에_거부한다() {
    let bundle = bundle_of(vec![file(SNAPSHOT_FILE, b"this is not a database at all")]);
    rejected(bundle, BundleProblem::CorruptDatabase).await;
}

// 검증: V51, P22
#[tokio::test]
async fn 되살린_폴더와_결과_어디에도_identity가_남지_않는다() {
    let env = Env::new().await;
    let (sealed, _) = uploaded(&env).await;
    let target = env.target("restored");
    let identity = env.identity_secret();

    let report = restore(&sealed, &identity, &target, LATER).unwrap();

    let secret = identity.expose().as_bytes();
    for bytes in all_bytes_under(&target) {
        assert!(!contains(&bytes, secret));
    }
    assert!(!format!("{report:?}").contains(identity.expose()));
    // 백업기 쪽 폴더에도 남기지 않는다.
    for bytes in all_bytes_under(env.dir.path()) {
        assert!(!contains(&bytes, secret));
    }
}
