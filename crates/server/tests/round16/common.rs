//! 9차 시험의 공용 도구: 가짜 바깥 명령 실행기, 보관 폴더 만들기, 파일 훑기.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};
use price_inha::{CommandLine, CommandOutput, CommandRunner, DB_FILE, Os, ServiceSetup, Settings};

/// 정의에 실릴 실행 파일 경로(가짜. 실제로 띄우지 않는다).
pub const EXE: &str = "/opt/price-inha/bin/price-inha";
/// 정의에 명시할 `PATH`.
pub const PATH_ENV: &str = "/usr/local/bin:/usr/bin:/bin";
/// macOS `gui/<uid>` 영역의 시험 uid.
pub const UID: u32 = 501;

/// 시험용 가짜 키 값들. 앞 4글자 · 뒤 4글자가 다른 곳에 나올 일이 없게 골랐다.
pub const BROKER_ID: &str = "qzbv-doctor-broker-id-wpkm";
pub const BROKER_SECRET: &str = "jxfr-doctor-broker-secret-tqhn";
pub const AI_VALUE: &str = "vkzq-doctor-ai-value-mxjd";
pub const SLOT_VALUE: &str = "hgpw-doctor-slot-value-zbqf";

/// 부른 명령 하나와 그때 정의 파일이 있었는지.
#[derive(Debug, Clone)]
pub struct Call {
    pub command: CommandLine,
    pub definition_existed: bool,
}

/// 부른 명령을 적어 두는 가짜 실행기. 실패시킬 명령을 고를 수 있다.
pub struct FakeRunner {
    calls: Mutex<Vec<Call>>,
    watch: Option<PathBuf>,
    fail: Box<dyn Fn(&CommandLine) -> bool + Send + Sync>,
}

impl FakeRunner {
    /// 모든 명령에 성공으로 답한다.
    pub fn ok() -> FakeRunner {
        FakeRunner::failing(|_| false)
    }

    /// `fail`이 참인 명령에는 실패로 답한다.
    pub fn failing(fail: impl Fn(&CommandLine) -> bool + Send + Sync + 'static) -> FakeRunner {
        FakeRunner {
            calls: Mutex::new(Vec::new()),
            watch: None,
            fail: Box::new(fail),
        }
    }

    /// 부를 때마다 `path`가 있었는지도 적는다.
    pub fn watching(mut self, path: &Path) -> FakeRunner {
        self.watch = Some(path.to_path_buf());
        self
    }

    /// 지금까지 부른 명령들.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// 지금까지 부른 명령 줄들.
    pub fn commands(&self) -> Vec<CommandLine> {
        self.calls().into_iter().map(|call| call.command).collect()
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, command: &CommandLine) -> io::Result<CommandOutput> {
        let definition_existed = self.watch.as_ref().is_some_and(|path| path.exists());
        self.calls.lock().unwrap().push(Call {
            command: command.clone(),
            definition_existed,
        });
        Ok(CommandOutput {
            success: !(self.fail)(command),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// 명령 줄을 글자 조각으로 만든다.
pub fn line(program: &str, args: &[&str]) -> CommandLine {
    CommandLine {
        program: program.to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
    }
}

/// 시험용 서비스 재료(정의 폴더는 임시 폴더, `--port 8123`을 함께 싣는다).
pub fn setup(os: Os, home: &Path, definitions: &Path) -> ServiceSetup {
    ServiceSetup {
        os,
        exe: PathBuf::from(EXE),
        home: home.to_path_buf(),
        definition_dir: definitions.to_path_buf(),
        uid: UID,
        path_env: PATH_ENV.to_string(),
        run_args: vec!["--port".to_string(), "8123".to_string()],
    }
}

/// 보관 폴더 `home`으로 읽은 설정(`--bind`는 `bind`, 포트 0).
pub fn settings(home: &Path, bind: &str) -> Settings {
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--bind",
        bind,
        "--port",
        "0",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    Settings::parse(&args, &|_| None, None).unwrap()
}

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

/// 건강한 보관 폴더를 만든다: 저장소 · 필수 키 · 칸 키 모두 등록, 폴더 0700 · 파일 0600.
pub async fn healthy_home(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", BROKER_ID)
        .with("client_secret", BROKER_SECRET);
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", AI_VALUE);
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    for slot in KeySlot::ALL {
        let values = Credentials::new().with(slot.fields()[0], SLOT_VALUE);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    drop(secrets);
    let store = pi_store::Store::open(home.join(DB_FILE)).unwrap();
    drop(store);
    narrow(home);
}

/// 보관 폴더를 0700, 안의 파일을 모두 0600으로 좁힌다(유닉스).
#[cfg(unix)]
pub fn narrow(home: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700)).unwrap();
    for file in files_under(home) {
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// 유닉스가 아니면 권한을 바꾸지 않는다.
#[cfg(not(unix))]
pub fn narrow(_home: &Path) {}

/// `dir` 아래(하위 폴더 포함)의 모든 파일.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// `dir` 아래 파일 가운데 `needle` 글자를 담은 것들.
pub fn files_containing(dir: &Path, needle: &str) -> Vec<PathBuf> {
    files_under(dir)
        .into_iter()
        .filter(|path| {
            let bytes = std::fs::read(path).unwrap_or_default();
            bytes
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
        })
        .collect()
}

/// 태그 사이의 빈칸(줄바꿈 · 들여쓰기)을 지운다. plist · XML 모양 차이에 흔들리지 않게.
pub fn squash(xml: &str) -> String {
    let mut out = String::new();
    let mut pending = String::new();
    for ch in xml.chars() {
        if ch.is_whitespace() {
            pending.push(ch);
            continue;
        }
        if ch != '<' || !out.ends_with('>') {
            out.push_str(&pending);
        }
        pending.clear();
        out.push(ch);
    }
    out
}
