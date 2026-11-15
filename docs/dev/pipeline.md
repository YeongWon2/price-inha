# 구현 파이프라인

모든 구현 작업은 이 순서로만 한다. 순서와 통과 조건은 고정이고, 바꾸려면 사용자에게 묻는다.

## 1. 층

| 층 | 묻는 것 | 여기서는 무엇인가 | 기계로 확인 |
|---|---|---|---|
| Prompt | 무엇을 말할까 | 노드마다 고정된 프롬프트 틀 `docs/dev/prompts/n1-test.md` · `n2-impl.md` · `n3-review.md`. `pipeline.py prompt <노드> <모듈>`이 틀에 모듈을 끼워 내준다 | 틀 밖의 말로 시키지 않는다 |
| Context | 무엇을 보여줄까 | `AGENTS.md`(짧은 지도) → `docs/product/`(스펙) → `docs/dev/modules/<모듈>.md`(그 모듈에 필요한 것만) | 지시서의 필수 번호가 테스트에 있는지 |
| Harness | 어떤 환경에서 일하게 할까 | Rust 워크스페이스, `scripts/red.sh`(G1), `scripts/verify.sh`(G2), `scripts/check_arch.py` + `docs/dev/architecture.toml`, clippy 규칙, 로컬 가짜 서버 | `scripts/selftest.py`가 검사기 자체를 시험한다 |
| Loop | 어떻게 되풀이할까 | 노드 안에서 "고친다 → 검사한다". `pipeline.py gate`가 실패를 세고 같은 실패 3번이면 멈춘다 | 실패 지문과 횟수 기록 |
| Graph | 어떻게 나누고 이을까 | `docs/dev/graph.toml`의 모듈 순서 · 병렬 · 노드별 권한. `scripts/pipeline.py`가 순서를 지키게 한다 | 앞 단계가 안 끝나면 시작을 거부, 맡지 않은 곳을 고치면 실패 |

## 2. 그래프

```text
N0 계획 ─→ N1 테스트 작성(red) ─→ G1 red 확인 ─→ N2 구현(green) ─→ G2 기계 검사 ─→ N3 AI 검토 ─→ G3 사람 확인
            (모듈마다, 병렬)       (기계)          (모듈마다, 병렬)   (기계)          (읽기 전용)     (커밋 · push)
```

| 노드 | 누가 | 받는 것 | 하는 일 | 내놓는 것 | 고칠 수 있는 곳 |
|---|---|---|---|---|---|
| N0 계획 | 조정자 | `dev.md`의 작업 번호, `docs/research/`의 조사 문서 | 모듈로 나누고 작업 지시서를 쓴다. 지시서마다 원본(Hermes · DeskRPG · OpenClaw)의 어느 동작을 따르는지 적는다. 조사 문서가 얇으면 먼저 더 뜯는다 | `docs/dev/modules/<모듈>.md` | `docs/dev/`, 워크스페이스 뼈대 |
| N1 테스트 작성 | 모듈마다 에이전트 1 | 작업 지시서 | 공개 API 뼈대(몸통은 `todo!()`)와 테스트를 **전부** 쓴다 | `crates/<모듈>/tests/*.rs`, 컴파일되는 뼈대 | `crates/<모듈>/` |
| G1 red 확인 | 기계 | | `scripts/red.sh <모듈>`: 컴파일되고, 테스트가 실패하고, 필수 번호가 다 있다 | `red/<모듈>` 태그(조정자가 커밋 뒤 붙임) | |
| N2 구현 | 모듈마다 에이전트 1 (N1과 다른 에이전트) | 작업 지시서, 잠긴 테스트 | 테스트가 통과할 때까지 구현한다 | `crates/<모듈>/src/` | `crates/<모듈>/src/`, 자기 `Cargo.toml` |
| G2 기계 검사 | 기계 | | `scripts/verify.sh` | 통과 또는 실패 목록 | |
| N3 AI 검토 | 구현하지 않은 에이전트 | 스펙, 지시서, 바뀐 코드 | 기계가 못 보는 것(스펙과 맞는가, 보안, 빠진 경우)을 판단한다 | 판정과 근거(`파일:줄`) | 없음 (읽기 전용) |
| G3 사람 확인 | 사용자 | 요약 | 커밋 · push를 정한다 | | |

## 3. 넘겨줄 때의 규칙

- N1과 N2는 서로 다른 에이전트다. 테스트를 쓴 쪽이 구현하지 않는다.
- G1을 지난 뒤 `crates/<모듈>/tests/`는 잠긴다. `verify.sh`가 `red/<모듈>` 태그와 비교해 바뀌면 실패시킨다.
- 테스트가 스펙과 다르거나 통과할 수 없게 쓰였으면 N2는 고치지 않고 멈춰 알린다. 조정자가 N1로 되돌린다.
- N3가 문제를 찾으면 N2로 돌아간다. N3는 직접 고치지 않는다.
- 병렬 노드는 자기 모듈 폴더만 고친다. 루트 파일과 다른 모듈은 건드리지 않는다.
- 원본 조사(그래프 밖의 일)는 `docs/research/`에만 쓴다. 노드의 범위 검사는 그 폴더를 보지 않는다(`graph.toml`의 `outside_graph`). 노드는 그 폴더를 고치지 않는다.
- 커밋은 실행기와 조정자만 한다. 실행기는 G1 잠금(`lock`)과 G2 통과 때 그 모듈 폴더만 커밋한다(한 번에 하나씩). 에이전트가 직접 `git commit`을 부르지 않는다.

## 4. 노드 안의 반복

- 목표: 그 노드의 통과 조건(G1 또는 G2).
- 같은 실패가 3번 되풀이되면 멈추고 사람에게 넘긴다. 무엇을 해 봤는지 적는다.
- 통과시키려고 검사를 끄지 않는다: `#[ignore]`, `#[allow(...)]`로 덮기, 테스트 지우기, `verify.sh` 고치기 금지.

## 5. 기계 검사 (G2, `scripts/verify.sh`)

| 번호 | 검사 | 실패 뜻 |
|---|---|---|
| 1 | `cargo fmt --all --check` | 모양이 다르다 |
| 2 | clippy (실행 코드는 엄격, 테스트는 `unwrap` 허용) | `unsafe` · `unwrap` · `expect` · `panic!` · `todo!` · `dbg!` · `println!` · 문서 주석 없는 공개 항목 등이 있다 |
| 3 | `cargo test --workspace` | 테스트가 깨진다 |
| 4 | 잠긴 테스트 비교 | red 뒤에 테스트를 고쳤다 |
| 5 | 필수 번호 | 지시서의 필수 번호가 테스트에 없다 |
| 5b | 원본 동작 근거 | 지시서에 "원본 동작 근거" 절이 없거나, 따르는 조사 문서를 가리키지 않는다(원본에 없는 기능이면 "원본에 없음"이라고 적어야 한다) |
| 6 | 아키텍처 · 컨벤션 (`scripts/check_arch.py`) | `docs/dev/architecture.toml`의 규칙을 어겼다: 의존 방향, 허용되지 않은 바깥 의존, 금지된 의존, 금지 모양(돈에 부동소수점, 주문 API 호출, 키 파일 직접 읽기, 라이브러리의 환경 변수 읽기 등), 구조(실행 파일 하나, `error.rs`), 크기(파일 · 함수 줄 수) |
| 7 | 비밀 글자 | 키처럼 생긴 글자나 `.env` · `auth.json`이 저장소에 있다 |
| 8 | 바깥 주소 | 테스트가 실제 증권사 · AI 주소를 쓴다 |
| 9 | 검사 끄기 | `#[ignore]`나 지시서에 없는 `#[allow]`가 있다 |

기계가 볼 수 있는 것은 모두 여기에 넣는다. AI 검토는 그 뒤에 남는 것만 판단한다.

어긴 곳은 `✗ [규칙 번호] 파일:줄`과 **왜** · **고치기**가 함께 찍힌다. 구현 에이전트는 그 안내대로 고치고 다시 돌린다.
규칙이 틀렸다고 보면 코드를 우회하지 말고 멈춰서 알린다. 규칙(`architecture.toml`, `Cargo.toml`의 lints, 검사 스크립트)은 조정자가 사용자 확인 뒤에만 고친다.
AI 검토에서 기계로 잡을 수 있는 종류의 문제가 나오면, 고치는 것과 함께 그 검사를 여기에 더한다.

## 6. 아키텍처와 컨벤션

규칙의 원본은 `docs/dev/architecture.toml`이다(사람과 기계가 같은 파일을 본다).

| 모듈 | 크레이트 | 맡은 구성 요소 | 써도 되는 내부 모듈 |
|---|---|---|---|
| store | `pi-store` | A12 저장소 | 없음 |
| secrets | `pi-secrets` | A13 비밀 보관 | 없음 |
| ai | `pi-ai` | A11 AI 공급자 어댑터 | `pi-secrets` |
| market | `pi-market` | A9 시장 어댑터 | `pi-secrets` |
| paper | `pi-paper` | A8 주문 실행기 가운데 모의계좌와 체결 | `pi-market` |
| gate | `pi-gate` | A2 접속 관문, 실시간 연결 | `pi-store` |
| server | `price-inha` | 실행 파일, 조립 | 모두 |
| template | `pi-template` | A6 가운데 템플릿 값 · 절대 상한 · 승격 기준 | 없음 |
| rules | `pi-rules` | A6 규칙 엔진 | `pi-template`, `pi-paper`, `pi-market` |
| approval | `pi-approval` | A7 승인 관리 | `pi-store` |
| exec | `pi-exec` | A8 주문 실행기(모의계좌 바깥) | `pi-paper`, `pi-market`, `pi-store` |
| data | `pi-data` | A10 자료 어댑터 | `pi-secrets` |
| trigger | `pi-trigger` | A5 트리거 감시 | `pi-market` |
| agent | `pi-agent` | A4 에이전트 실행기 | `pi-ai`, `pi-store` |
| journal | `pi-journal` | A4 복기 · M4 · 하루 정리 | `pi-store`, `pi-template` |
| knowledge | `pi-knowledge` | A12 지식 검색 | `pi-store` |
| backup | `pi-backup` | A14 백업기 | `pi-store`, `pi-secrets` |
| office | `pi-office` | A3 사무실 서버 + A1 3D 사무실 화면 | `pi-store` |
| notify | `pi-notify` | A3 알림 · 웹 푸시 + A1 폰 화면 | `pi-store` |

새 모듈(template 아래 12개)은 2026-11-16 로드맵(`docs/dev/roadmap.md`)으로 더했다. 표의 행은 계획이고, `architecture.toml`의 `[crates.*]` 항목은 그 크레이트를 처음 만드는 N1 커밋과 함께 더한다(없는 크레이트가 표에 있으면 ARCH-CRATE가 모든 모듈 검사를 깨기 때문이다).
**새 모듈을 더하는 절차** (N0, 조정자): (1) `architecture.toml`에 그 크레이트 항목(과 새 금지 모양, 그 표본을 `scripts/selftest.py` `PATTERN_SAMPLES`에)을 더하고 `[crates.price-inha] internal`에 이름을 더한다. (2) 같은 커밋에 워크스페이스 뼈대 `crates/<모듈>/`(`Cargo.toml`, 문서 주석만 있는 `src/lib.rs`, `src/error.rs`)를 만든다(워크스페이스는 `crates/*`를 이미 구성원으로 둔다). (3) `python3.14 scripts/selftest.py`를 돌려 통과시키고 커밋한다. (4) 그다음 N1을 시작한다. 검사 스크립트(`scripts/lib.sh`)는 `crates/<모듈>/Cargo.toml`이 있는 모듈만 검사한다. 그래서 뼈대를 만든 뒤 G1 전까지는 전체 검사(`scripts/verify.sh`)가 그 모듈의 필수 번호 빠짐으로 실패한다. 이 사이에는 모듈별 검사(`scripts/verify.sh <모듈>`)를 쓴다.
조립(`crates/server`)은 2026-11-16부터 gate가 아니라 server 모듈이다(`docs/dev/modules/server.md`).

컨벤션 요약: 오류는 모듈마다 `src/error.rs`의 `thiserror` enum, 공개 항목에는 문서 주석, 라이브러리는 환경 변수 · 시계를 직접 읽지 않고 인자로 받음, 기록은 `tracing`(`println!` 금지), 돈은 십진수, 파일 500줄 · 함수 80줄 이하.

## 7. 필수 번호를 테스트에 적는 법

테스트 함수 바로 위에 주석으로 적는다. 한 테스트에 여러 개를 적어도 된다.

```rust
// 검증: N8, E4
#[test]
fn 확인에_실패한_키는_저장하지_않는다() { ... }
```

지시서의 `## 필수 번호` 줄에 있는 번호가 그 모듈 `tests/` 어디에도 없으면 G1 · G2가 실패한다.

## 8. 실행기 (`scripts/pipeline.py`)

조정자는 이 명령으로만 그래프를 나아가게 한다. 상태는 `.pipeline/state.json`에 있다.

| 명령 | 하는 일 |
|---|---|
| `status` | 모듈마다 지금 단계와 다음에 할 수 있는 것 |
| `next` | 지금 시작할 수 있는 노드(여러 개면 병렬로 돌린다) |
| `prompt <노드> <모듈>` | 그 노드에 줄 프롬프트 |
| `start <노드> <모듈>` | 노드 시작. 앞 단계가 안 끝났으면 거부한다 |
| `gate <G1\|G2> <모듈>` | 기계 검사와 권한 검사를 돌리고 기록한다. 같은 실패 3번이면 사람에게 넘긴다 |
| `lock <모듈>` | 테스트를 커밋하고 `red/<모듈>` 태그로 잠근다. `gate G1`이 통과하면 실행기가 바로 부른다(잠그기 직전에 red를 한 번 더 확인한다) |
| `review <모듈> <통과\|되돌림>` | AI 검토 판정. 되돌림이면 N2로 돌아가고, 2번을 넘으면 사람에게 넘긴다 |
| `scope` | 진행 중인 노드가 맡지 않은 곳을 고쳤는지 |
| `resume <모듈>` | 사람에게 넘어간 모듈을 사람이 다시 연다 |

## 9. 검사기 자체 시험 (`scripts/selftest.py`)

검사기가 위반을 실제로 잡는지 확인한다. 깨끗한 가짜 저장소를 만들어 규칙을 하나씩 어기고, 그 규칙 이름으로 잡히는지 본다. 깨끗할 때 통과하는지도 본다.

- 규칙(`architecture.toml`), 검사 스크립트, 그래프, 루트 `Cargo.toml` · `clippy.toml`을 고치면 자체 시험을 다시 돌려야 한다. `verify.sh`의 0번 검사가 규칙의 지문이 마지막 자체 시험 때와 같은지 본다.
- `architecture.toml`에 금지 모양을 더하면 `selftest.py`의 표본에도 "어긴 코드 한 줄"을 더해야 한다. 없으면 자체 시험이 실패한다.
