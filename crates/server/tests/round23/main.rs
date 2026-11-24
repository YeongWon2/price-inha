//! 20차(`docs/dev/modules/server.md` "20차 사무실을 원본 자산으로" · "20차 고침")의 Rust 쪽 시험.
//!
//! - `assets`: 1번(원본 DeskRPG 자산이 `web/public`과 `web/dist`에 해시 표와 같은 바이트로 있고 SOURCE.md가 있다,
//!   들이지 않는 것이 없다, 서버가 같은 출처 경로로 내준다).
//! - `bundle`: 1번 · 2번 · 5번(사무실 3D 묶음이 외형 GLB를 같은 출처 경로로 부른다, 바깥 주소가 없다,
//!   카드 · 폰 · 설정 · 키 묶음은 GLB를 부르지 않는다).
//! - `looks`: 4번(담당 외형 저장 API).
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄우고 바깥은 아무도 듣지 않는 로컬 주소다.
//! 해시 표 `deskrpg_assets.txt`는 원본 사본에서 계산해 이 폴더에 둔 데이터다(시험이 임시 경로에 기대지 않는다).

mod assets;
mod bundle;
mod common;
mod looks;
#[path = "../support/built.rs"]
mod screen_built;
#[path = "../support/dist.rs"]
mod screen_dist;
#[path = "../support/refs.rs"]
mod screen_refs;
