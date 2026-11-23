# 구현 파이프라인

모든 구현 작업은 이 순서로만 한다. 순서와 통과 조건은 고정이고, 바꾸려면 사용자에게 묻는다.

## 1. 층

| 층 | 묻는 것 | 여기서는 무엇인가 | 기계로 확인 |
|---|---|---|---|
| Prompt | 무엇을 말할까 | 노드마다 고정된 프롬프트 틀 `docs/dev/prompts/r-reconcile.md` · `n1-test.md` · `n2-impl.md` · `n3-review.md`. `pipeline.py prompt <노드> <모듈>`이 틀에 모듈을 끼워 내준다 | 틀 밖의 말로 시키지 않는다 |
| Context | 무엇을 보여줄까 | `AGENTS.md`(짧은 지도) → `docs/product/`(스펙) → `docs/dev/modules/<모듈>.md`(그 모듈에 필요한 것만) | 지시서의 필수 번호가 테스트에 있는지 |
| Harness | 어떤 환경에서 일하게 할까 | Rust 워크스페이스, `scripts/red.sh`(G1), `scripts/verify.sh`(G2), `scripts/check_arch.py` + `docs/dev/architecture.toml`, clippy 규칙, 로컬 가짜 서버 | `scripts/selftest.py`가 검사기 자체를 시험한다 |
| Loop | 어떻게 되풀이할까 | 노드 안에서 "고친다 → 검사한다". `pipeline.py gate`가 실패를 세고 같은 실패 2번이면 모델 승급을 알리고, 3번이면 멈춘다 | 실패 지문과 횟수 기록 |
| Graph | 어떻게 나누고 이을까 | `docs/dev/graph.toml`의 모듈 순서 · 병렬 · 노드별 권한. `scripts/pipeline.py`가 순서를 지키게 한다 | 앞 단계가 안 끝나면 시작을 거부, 맡지 않은 곳을 고치면 실패 |

## 2. 그래프

```text
N0 계획 ─→ [R 대조 ─→ GR 표 확인] ─→ N1 테스트 작성(red) ─→ G1 red 확인 ─→ N2 구현(green) ─→ G2 기계 검사 ─→ N3 AI 검토 ─→ G3 사람 확인
            (다시 연 모듈만)          (모듈마다, 병렬)       (기계)          (모듈마다, 병렬)   (기계)          (읽기 전용)     (커밋 · push)
```

실행기 단계: 할 일 → (다시 연 모듈만: R 대조 중 → 대조 확인됨) → N1 테스트 작성 중 → red 확인됨 → 테스트 잠김 → N2 구현 중 → green → N3 AI 검토 중 → 검토 통과.

| 노드 | 누가 | 받는 것 | 하는 일 | 내놓는 것 | 고칠 수 있는 곳 |
|---|---|---|---|---|---|
| N0 계획 | 조정자 | `dev.md`의 작업 번호, `docs/research/`의 조사 문서 | 모듈로 나누고 작업 지시서를 쓴다. 지시서마다 원본(Hermes · DeskRPG · OpenClaw)의 어느 동작을 따르는지 적는다. 조사 문서가 얇으면 먼저 더 뜯는다 | `docs/dev/modules/<모듈>.md` | `docs/dev/`, 워크스페이스 뼈대 |
| R 대조 (다시 연 모듈만) | 모듈마다 에이전트 1 (N1과 다른 에이전트) | 지시서의 이번 차수 절, 잠겨 있던 시험, 결정 · 사용자 말 | 낡을 수 있는 시험 · 문서마다 유지 · 고침 · 지움 · 열어 둠을 정한다 | 지시서 이번 차수 절 아래 `### 대조` 표(시험 · 문서 경로 \| 판정 \| 근거) | `docs/dev/modules/<모듈>.md`뿐 |
| GR 표 확인 | 기계 | | `pipeline.py gate GR <모듈>`: 이번 차수의 대조 표가 있고(다시 연 때보다 `### 대조`가 늘었다), 판정이 넷 가운데 하나이고, "지움" 줄마다 근거 칸에 `사용자 "…"` · `결정 <번호>` · `지시서 <N>차` 가운데 하나가 있다(형식 검사) | 지시서 커밋(실행기) | |
| N1 테스트 작성 | 모듈마다 에이전트 1 | 작업 지시서 | 공개 API 뼈대(몸통은 `todo!()`)와 테스트를 **전부** 쓴다 | `crates/<모듈>/tests/*.rs`, 컴파일되는 뼈대 | `crates/<모듈>/` |
| G1 red 확인 | 기계 | | `scripts/red.sh <모듈>`: 컴파일되고, 테스트가 실패하고, 필수 번호가 다 있다. 다시 연 모듈은 잠금 판(`red/<모듈>`이 가리키던 판) 대비 지운 시험 파일 · 시험 함수가 대조 표의 "지움"에 다 있어야 한다(없으면 "대조 표에 없는 시험을 지웠다"). 동작 잠금 모드면 "실패한다" 대신 "지금 구현에서 모두 통과한다" | `red/<모듈>` 태그(조정자가 커밋 뒤 붙임) | |
| N2 구현 | 모듈마다 에이전트 1 (N1과 다른 에이전트) | 작업 지시서, 잠긴 테스트 | 테스트가 통과할 때까지 구현한다 | `crates/<모듈>/src/` | `crates/<모듈>/src/`, 자기 `Cargo.toml` |
| G2 기계 검사 | 기계 | | `scripts/verify.sh` | 통과 또는 실패 목록 | |
| N3 AI 검토 | 구현하지 않은 에이전트 | 스펙, 지시서, 바뀐 코드 | 기계가 못 보는 것(스펙과 맞는가, 보안, 빠진 경우)을 판단한다 | 판정과 근거(`파일:줄`) | 없음 (읽기 전용) |
| G3 사람 확인 | 사용자 | 요약 | 커밋 · push를 정한다 | | |

**동작 잠금 모드** (QMIT 결정 2): 리팩토링 · 옮기기처럼 동작을 바꾸지 않는 차수는 red 대신 "바꾸기 전과 뒤 모두 통과"로 증명한다. `pipeline.py reopen <모듈> --lock <이유>`(또는 `start N1 <모듈> --lock`)로 그 차수를 동작 잠금 모드로 둔다. 그러면 G1(`red.sh <모듈> --lock`)은 시험이 **지금 구현에서 모두 통과**해야 통과하고, N2 뒤 G2도 당연히 통과해야 한다. 모드는 상태 파일(`mode`)에 남고 `status`에 "동작 잠금"으로 보인다.

**크기별 경로** (QMIT 결정 2, 규칙만 — 기계화하지 않는다):
- 조사 · 질문만이면 파이프라인을 열지 않는다(`reopen` · `start`를 하지 않는다). 코드를 바꾸지 않고 답하고 끝낸다.
- 작은 변경(한 모듈 안, 공개 API · 계약이 그대로, 필수 번호 몇 개)은 지시서의 차수 절에서 설계 설명을 줄여 "만드는 것"과 필수 번호만 적는다. 그래프(R → N1 → N2 → N3)는 그대로 돈다.
- 큰 변경(새 모듈 · 계약 변경 · 여러 모듈)은 지금처럼 N0 계획부터 다 쓴다.

## 3. 넘겨줄 때의 규칙

- N1과 N2는 서로 다른 에이전트다. 테스트를 쓴 쪽이 구현하지 않는다.
- R(대조)과 N1은 서로 다른 에이전트다. "지워서 초록불"을 같은 쪽이 스스로 승인하지 못하게 한다. `reopen`한 모듈은 R부터 돌고(`start N1`이 거부한다), 처음 짓는 모듈은 R 없이 N1부터 돈다.
- N3(검토)는 가능하면 N2(구현)와 다른 모델로 맡긴다.
- 위임 예산: 동시에 도는 에이전트는 4개 이하다. 같은 일의 재위임(실패 뒤 다른 에이전트 · 모델로 다시 맡기기)은 1번까지이고, 그래도 안 되면 사람에게 넘긴다.
- G1을 지난 뒤 `crates/<모듈>/tests/`는 잠긴다. `verify.sh`가 `red/<모듈>` 태그와 비교해 바뀌면 실패시킨다.
- 테스트가 스펙과 다르거나 통과할 수 없게 쓰였으면 N2는 고치지 않고 멈춰 알린다. 조정자가 N1로 되돌린다.
- N3가 문제를 찾으면 N2로 돌아간다. N3는 직접 고치지 않는다.
- 새 바깥 의존(Rust · npm 새 이름)이 필요하면 N2는 더하지 않고 멈춰 보고한다. 조정자가 사용자 승인을 받아 더한다(5장 공급망 규칙, QMIT 결정 8).
- 병렬 노드는 자기 모듈 폴더만 고친다. 루트 파일과 다른 모듈은 건드리지 않는다.
- 원본 조사(그래프 밖의 일)는 `docs/research/`에만 쓴다. 노드의 범위 검사는 그 폴더를 보지 않는다(`graph.toml`의 `outside_graph`). 노드는 그 폴더를 고치지 않는다.
- 커밋은 실행기와 조정자만 한다. 실행기는 G1 잠금(`lock`)과 G2 통과 때 그 모듈 폴더만 커밋한다(한 번에 하나씩). 에이전트가 직접 `git commit`을 부르지 않는다.

## 4. 노드 안의 반복

- 목표: 그 노드의 통과 조건(G1 또는 G2).
- 같은 실패가 2번이면 `gate`가 "같은 실패 2번 — 이 노드를 한 등급 위 모델로 다시 맡긴다(Haiku→Sonnet→Opus→Fable…)"를 찍고 상태에 `escalate_model: true`를 남긴다(`status`에 "모델 승급"). 조정자는 그 노드를 한 등급 위 모델로 다시 맡긴다(이것이 재위임 1번이다). 한도는 `graph.toml`의 `escalate_model_at`.
- 같은 실패가 3번 되풀이되면 멈추고 사람에게 넘긴다. 무엇을 해 봤는지 적는다.
- 통과시키려고 검사를 끄지 않는다: `#[ignore]`, `#[allow(...)]`로 덮기, 테스트 지우기, `verify.sh` 고치기 금지.

## 5. 기계 검사 (G2, `scripts/verify.sh`)

| 번호 | 검사 | 실패 뜻 |
|---|---|---|
| 1 | `cargo fmt --all --check` | 모양이 다르다 |
| 2 | clippy (실행 코드는 엄격, 테스트는 `unwrap` 허용) | `unsafe` · `unwrap` · `expect` · `panic!` · `todo!` · `dbg!` · `println!` · 문서 주석 없는 공개 항목 등이 있다 |
| 3 | `cargo test --workspace` | 테스트가 깨진다 |
| 3w | 화면(Vite, `crates/<모듈>/web/package.json`이 있을 때): `npm ci` · `npm run typecheck` · `npm test`(vitest) · `npm run build` 뒤 `web/dist`가 그대로인가 | 화면 형 오류 · 화면 시험 실패 · 커밋한 `dist`가 소스와 다르다(빌드 결과를 함께 커밋한다). 화면 시험은 `crates/<모듈>/tests/web/*.test.ts`에 두어 잠금이 같이 덮는다. G1(red)은 Rust 시험이나 화면 시험 가운데 하나라도 실패하면 red다 |
| 4 | 잠긴 테스트 비교 | red 뒤에 테스트를 고쳤다 |
| 5 | 필수 번호 | 지시서의 필수 번호가 테스트에 없다 |
| 5b | 원본 동작 근거 | 지시서에 "원본 동작 근거" 절이 없거나, 따르는 조사 문서를 가리키지 않는다(원본에 없는 기능이면 "원본에 없음"이라고 적어야 한다) |
| 6 | 아키텍처 · 컨벤션 (`scripts/check_arch.py`) | `docs/dev/architecture.toml`의 규칙을 어겼다: 의존 방향, 허용되지 않은 바깥 의존, 금지된 의존, 금지 모양(돈에 부동소수점, 주문 API 호출, 키 파일 직접 읽기, 라이브러리의 환경 변수 읽기 등), 구조(실행 파일 하나, `error.rs`), 크기(파일 · 함수 줄 수) |
| 7 | 비밀 글자 | 키처럼 생긴 글자나 `.env` · `auth.json`이 저장소에 있다 |
| 8 | 바깥 주소 | 테스트가 실제 증권사 · AI 주소를 쓴다 |
| 9 | 검사 끄기 | `#[ignore]`나 지시서에 없는 `#[allow]`가 있다 |
| 10 | 공급망 (`cargo deny --offline check licenses bans sources`, 설정은 루트 `deny.toml`) | 허용 목록 밖 라이선스, crates.io 밖 출처(git · 다른 레지스트리), 금지 크레이트(`openssl-sys` · `native-tls`)가 의존 나무에 있다. cargo-deny가 없으면 ✗와 설치 안내(`cargo install cargo-deny --version <scripts/lib.sh DENY_VERSION> --locked`) |

기계가 볼 수 있는 것은 모두 여기에 넣는다. AI 검토는 그 뒤에 남는 것만 판단한다.

**영향 범위** (QMIT 결정 6): `scripts/verify.sh --affected [--base <ref>]`는 `scripts/affected.py`가 고른 모듈만 돈다. 기준(기본 `origin/main`, 없으면 `HEAD~1`)과 작업 트리(커밋 안 된 것 포함)의 차이에서 바뀐 `crates/<모듈>/`과 그 크레이트에 (전이적으로) 기대는 크레이트(`cargo metadata`의 path 의존), 바뀐 지시서 `docs/dev/modules/<모듈>.md`의 모듈을 고른다. 그래프가 못 따라가는 변경(루트 `Cargo.toml` · 판이 바뀐 `Cargo.lock` · `scripts/` · `architecture.toml` · `graph.toml` · `clippy.toml` · `.cargo/` · `.github/`, 그 밖에 모르는 곳)은 전체를 돈다. 문서만 바뀌었으면 "바뀐 코드 없음"으로 0이다. CI는 PR에서 `--affected --base origin/<대상 브랜치>`, main push · 밤(매일 03:00 KST) · 손으로 돌릴 때 전체를 돈다.

**로컬 pre-push 관문** (QMIT 결정 4): `.githooks/pre-push`가 push되는 범위(원격 sha 기준, 새 브랜치면 `origin/main`)로 `verify.sh --affected`를 돌리고 실패하면 push를 막는다. 저장소마다 한 번 `git config core.hooksPath .githooks`로 켠다(꺼져 있으면 `verify.sh`가 참고 줄로 알린다). 관문을 건너뛰는 push는 하지 않는다.

**공급망 규칙** (QMIT 결정 8):
- **새 바깥 의존은 사용자 승인으로만.** 새 바깥 의존(루트 `Cargo.toml` `[workspace.dependencies]`에 새 이름, `crates/*/web/package.json`에 새 이름)은 N2(와 다른 노드)가 더하지 않고 멈춰 보고한다. 조정자가 사용자 승인을 받아 `architecture.toml`(그 크레이트의 `external`, 테스트 · 실행 파일 전용이면 `[dep_workspace] extra`, 화면이면 `[web] allowed`)과 의존을 같은 커밋에 더한다. 기계로는 6번 검사가 막는다: `[workspace.dependencies]`의 바깥 이름이 크레이트별 `external`의 합(`price-inha`의 `"*"`는 빼고)과 `[dep_workspace] extra` 밖이면 ARCH-DEP-NEW, `package.json`의 의존 이름이 `[web] allowed` 밖이면 ARCH-WEB-DEP.
- **오프라인(G2)**: 10번 검사(cargo-deny licenses · bans · sources). 라이선스 허용 목록 · 예외는 `deny.toml`에 있고, 바꾸는 것은 조정자가 사용자 확인 뒤에만 한다.
- **네트워크(G2 밖)**: `.github/workflows/audit.yml`이 보안 공지(`cargo audit` · `cargo deny check advisories`, 무시 목록은 `.cargo/audit.toml`과 `deny.toml`에 같은 사유로)와 새 판 발행일(`scripts/supply.py`)을 본다. PR · push는 기준 판에 없던 새 판만 보고 `[supply] min_age_days`(14일)보다 새것이면 실패, 밤(매일 02:00 KST)은 `--all`로 모든 판을 보고만 한다.
- **발행일 검사** `python3.14 scripts/supply.py [--since <ref>] [--head <ref>] [--all]`: `Cargo.lock`의 crates.io 패키지와 `crates/*/web/package-lock.json`의 npm 패키지의 발행 시각을 crates.io · npm 레지스트리에서 받는다(동시 4개 이하, 캐시 `.pipeline/supply-cache.json`). 기본 기준은 `origin/main`. 종료 0 통과 · 1 너무 새 판 · 2 기준 오류 · 3 발행일을 받지 못함(네트워크). `--offline-fixture <json>`은 자체 시험용 가짜 발행일 표다.
- **pre-push**: push 범위에서 `Cargo.lock`이나 `package-lock.json`이 바뀌었을 때만 `supply.py --since <원격 sha> --head <로컬 sha>`를 돌린다. 너무 새 판이면 push를 막고, 네트워크가 없으면(종료 3) 경고만 한다.

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

의존의 판은 루트 `Cargo.toml`의 `[workspace.dependencies]` 한 곳에서 정한다(QMIT 결정 5). 크레이트에는 `<이름> = { workspace = true }`(기능은 `features = [...]`로 더함)로만 적고, 판(문자열이나 `version =`)을 직접 적으면 ARCH-DEP-WORKSPACE로 막는다. 크레이트끼리 판이 갈려 사람에게 묻는 중인 것만 `architecture.toml`의 `[dep_workspace] pending`에 둔다.

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
| `start <노드> <모듈> [--lock]` | 노드 시작(`R` · `N1` · `N2` · `N3`). 앞 단계가 안 끝났으면 거부한다. 다시 연 모듈의 `N1`은 GR 통과 전이면 거부한다. `N1 --lock`은 동작 잠금 모드 |
| `gate <GR\|G1\|G2> <모듈>` | 기계 검사와 권한 검사를 돌리고 기록한다. GR이 통과하면 지시서를 커밋한다. 같은 실패 2번이면 모델 승급 안내, 3번이면 사람에게 넘긴다 |
| `lock <모듈>` | 테스트를 커밋하고 `red/<모듈>` 태그로 잠근다. `gate G1`이 통과하면 실행기가 바로 부른다(잠그기 직전에 red를 한 번 더 확인한다) |
| `review <모듈> <통과\|되돌림>` | AI 검토 판정. 되돌림이면 N2로 돌아가고, 2번을 넘으면 사람에게 넘긴다 |
| `scope` | 진행 중인 노드가 맡지 않은 곳을 고쳤는지 |
| `resume <모듈>` | 사람에게 넘어간 모듈을 사람이 다시 연다 |
| `reopen <모듈> [--lock] <이유>` | 조정자가 모듈을 다시 연다(새 차수 · 테스트 결함). 잠금 태그를 지우고, 그 판을 `base`로 남기고, R(대조)부터 돌게 한다. `--lock`은 동작 잠금 모드 |
| `amend <모듈> <이유>` | 뜻이 안 바뀌는 테스트 결함(이름 · 모양)을 조정자가 바로잡고 다시 잠근다 |

## 9. 검사기 자체 시험 (`scripts/selftest.py`)

검사기가 위반을 실제로 잡는지 확인한다. 깨끗한 가짜 저장소를 만들어 규칙을 하나씩 어기고, 그 규칙 이름으로 잡히는지 본다. 깨끗할 때 통과하는지도 본다.

- 규칙(`architecture.toml`), 검사 스크립트, 그래프, 루트 `Cargo.toml` · `clippy.toml`을 고치면 자체 시험을 다시 돌려야 한다. `verify.sh`의 0번 검사가 규칙의 지문이 마지막 자체 시험 때와 같은지 본다.
- `architecture.toml`에 금지 모양을 더하면 `selftest.py`의 표본에도 "어긴 코드 한 줄"을 더해야 한다. 없으면 자체 시험이 실패한다.
