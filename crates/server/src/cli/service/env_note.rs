//! (65차, 9차 보강 4) 환경 변수로 준 `--bind` · `--port`는 서비스 정의에 실리지 않는다는 안내.
//!
//! 정의는 `run --home <홈> [준 인자들]`과 `PATH`만 싣는다(비밀 값 · 그 밖의 환경은 싣지 않는다). 그래서 등록할 때
//! 환경 변수로만 준 주소 · 포트는 서비스로 띄우면 사라진다. 인자로도 줬으면(정의에 실린다) 알리지 않는다.

use crate::settings::{ENV_BIND, ENV_PORT};

/// 환경 변수 이름 · 그 값을 실을 인자.
const CARRIED: [(&str, &str); 2] = [(ENV_BIND, "--bind"), (ENV_PORT, "--port")];

/// `lookup`(환경 변수 읽기)으로 본 주소 · 포트 가운데 `run_args`에 없는 것마다 안내 한 줄.
pub fn service_env_notes(lookup: &dyn Fn(&str) -> Option<String>, run_args: &[String]) -> Vec<String> {
    let _ = (lookup, run_args, CARRIED);
    Vec::new() // red 뼈대
}
