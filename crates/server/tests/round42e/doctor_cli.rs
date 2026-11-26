//! `price-inha doctor`(CLI)를 시험 보관 폴더로 돌리는 도구. 바깥 명령은 가짜 실행기(늘 성공)뿐이다.

use std::io;
use std::path::{Path, PathBuf};

use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};
use price_inha::{
    CheckItem, CommandLine, CommandOutput, CommandRunner, InstallOptions, Os, Service,
    ServiceSetup, Settings, doctor,
};

/// 무엇이든 받는 시험용 확인자(바깥으로 나가지 않는다).
pub struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 늘 성공으로 답하는 가짜 바깥 명령 실행기.
struct OkRunner;

impl CommandRunner for OkRunner {
    fn run(&self, _command: &CommandLine) -> io::Result<CommandOutput> {
        Ok(CommandOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// 필수 키(증권사 · AI)를 등록한 보관 폴더.
pub async fn home_with_keys(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "qzbv-round42e-broker-id-wpkm")
        .with("client_secret", "jxfr-round42e-broker-secret-tq");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "vkzq-round42e-ai-value-mxjd");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
}

/// `home`을 진단하고 키 항목의 "본 것" 글을 돌려준다.
pub fn keys_line(home: &Path) -> String {
    let defs = tempfile::tempdir().unwrap();
    let setup = ServiceSetup {
        os: Os::MacOs,
        exe: PathBuf::from("/opt/price-inha/bin/price-inha"),
        home: home.to_path_buf(),
        definition_dir: defs.path().to_path_buf(),
        uid: 501,
        path_env: "/usr/local/bin:/usr/bin:/bin".to_string(),
        run_args: vec!["--port".to_string(), "8123".to_string()],
    };
    let runner = OkRunner;
    let service = Service::new(setup, &runner);
    service
        .install(InstallOptions {
            start_now: false,
            dry_run: false,
        })
        .unwrap();
    let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0"]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
    let settings = Settings::parse(&args, &|_| None, None).unwrap();
    let report = doctor(&settings, &service);
    report
        .check(CheckItem::Keys)
        .expect("키 항목이 있다")
        .what
        .clone()
}
