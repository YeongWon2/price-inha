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
| N1 테스트 작성 | 모듈마다 에이전트 1 | 작업 지시서 | 공개 API 뼈대(몸통은 `todo!()`)와 테스트를 **전부** 쓴다. 화면이 바뀌는 차수면 브라우저 e2e 시험(`crates/<모듈>/tests/e2e/*.spec.ts`)도 쓴다 — red 때는 돌리지 않고(문법 · 형 · 시험 린트만) G2에서 돈다. 영상은 찍지 않는다(결정 23) | `crates/<모듈>/tests/*.rs`, 컴파일되는 뼈대 | `crates/<모듈>/` |
| G1 red 확인 | 기계 | | `scripts/red.sh <모듈>`: 컴파일되고, 테스트가 실패하고, 필수 번호가 다 있다. 화면이 있으면 화면 형 검사(tsc: 화면 · vitest 시험 · e2e 시험)와 시험 린트도 통과해야 한다(e2e는 돌리지 않는다). 다시 연 모듈은 잠금 판(`red/<모듈>`이 가리키던 판) 대비 지운 시험 파일 · 시험 함수가 대조 표의 "지움"에 다 있어야 한다(없으면 "대조 표에 없는 시험을 지웠다"). 동작 잠금 모드면 "실패한다" 대신 "지금 구현에서 모두 통과한다" — 이때는 다시 연 판 대비 **바뀐 · 새 시험 대상만** 돌린다(src · 기대는 크레이트 · 빌드 파일이 바뀌었거나 대상을 모르는 시험 파일이 바뀌면 모듈 전부. 나머지는 G2가 모듈 전체를 돈다) | `red/<모듈>` 태그(조정자가 커밋 뒤 붙임) | |
| N2 구현 | 모듈마다 에이전트 1 (N1과 다른 에이전트) | 작업 지시서, 잠긴 테스트 | 테스트가 통과할 때까지 구현한다 | `crates/<모듈>/src/` | `crates/<모듈>/src/`, 자기 `Cargo.toml` |
| G2 기계 검사 | 기계 | | `scripts/verify.sh --affected --base <그 차수가 열린 커밋> --focus <모듈> [--wip <작업 중인 모듈>]`(5장 영향 범위 · 캐시). 차수가 열린 커밋을 모르면 `scripts/verify.sh <모듈>` | 통과 또는 실패 목록 | |
| N3 AI 검토 | 구현하지 않은 에이전트 | 스펙, 지시서, 바뀐 코드 | 기계가 못 보는 것(스펙과 맞는가, 보안, 빠진 경우)을 판단한다. 화면 확인은 영상이 아니라 G2의 브라우저 e2e 결과로 본다 — 화면이 바뀌었는데 e2e 시험이 그 흐름을 덮지 않으면 빠진 경우로 적는다(결정 23) | 판정과 근거(`파일:줄`) | 없음 (읽기 전용) |
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
| 3 | `cargo test` (`scripts/cargo_tests.py`: 빌드는 한 번, 실행 · 결과는 모듈마다. 전체 범위는 모든 모듈, 영향 범위는 아래 규칙의 모듈) | 테스트가 깨진다. 실패한 시험이 5개 이하면 그 시험 하나만 곧바로 혼자 한 번 다시 돌려, 통과하면 "흔들림: <이름> — 혼자 다시 돌려 통과"로 적고 통과로 친다(`.pipeline/flaky.json`에 횟수, `pipeline.py flaky`로 고칠 후보를 본다). 다시 돌려도 실패하면 실패 |
| 3w | 화면(Vite, `crates/<모듈>/web/package.json`이 있을 때): `npm ci` · `npm run typecheck` · `npm test`(vitest) · `npm run build` 뒤 `web/dist`가 그대로인가 | 화면 형 오류 · 화면 시험 실패 · 커밋한 `dist`가 소스와 다르다(빌드 결과를 함께 커밋한다). 화면 시험은 `crates/<모듈>/tests/web/*.test.ts`에 두어 잠금이 같이 덮는다. G1(red)은 Rust 시험이나 화면 시험 가운데 하나라도 실패하면 red다 |
| 3l | 화면 린트 (`npm run lint`: ESLint + typescript-eslint, 규칙은 `crates/<모듈>/web/eslint.config.js`, 결정 23) | 실행 코드(`web/src`)에서 형 정보를 쓰는 규칙(recommendedTypeChecked · 기다리지 않은 Promise · Promise 오용 · `any` · `==` · 안 쓰는 매개변수)이나 금지 속성(`innerHTML` · `outerHTML` · `insertAdjacentHTML` · `document.write` · `eval` · `localStorage` · `sessionStorage` · `indexedDB` · `document.cookie`)에 걸린다. 시험(`tests/web` · `tests/e2e`)은 느슨한 묶음(기다리지 않은 Promise · `==`). 인라인 주석으로 규칙을 끌 수 없다(`noInlineConfig`) |
| 3e | 브라우저 e2e (`npm run e2e`: Playwright, chromium headless 하나 · 병렬 1 · 재시도 0, 결정 23) | 시험(`crates/<모듈>/tests/e2e/*.spec.ts`)이 실패한다. 전역 준비가 임시 홈으로 시연 서버(가짜 시세 · 가짜 AI · 모의계좌)를 빈 포트에 띄우고 끝나면 지운다. 모든 시험에서 화면 오류(`pageerror`)와 바깥 요청을 실패로 보고, 서버가 바깥으로 나가려던 요청(기록 프록시)이 있으면 실패로 본다. chromium이 없으면 실패 — 고치기 `npx playwright install chromium`. trace · 스크린샷은 저장소 밖 임시 폴더(`$TMPDIR/price-inha-e2e`)에만. 영상은 찍지 않는다 |
| 4 | 잠긴 테스트 비교 | red 뒤에 테스트를 고쳤다 |
| 5 | 필수 번호 | 지시서의 필수 번호가 테스트에 없다 |
| 5b | 원본 동작 근거 | 지시서에 "원본 동작 근거" 절이 없거나, 따르는 조사 문서를 가리키지 않는다(원본에 없는 기능이면 "원본에 없음"이라고 적어야 한다) |
| 6 | 아키텍처 · 컨벤션 (`scripts/check_arch.py`) | `docs/dev/architecture.toml`의 규칙을 어겼다: 의존 방향, 허용되지 않은 바깥 의존, 금지된 의존, 금지 모양(돈에 부동소수점, 주문 API 호출, 키 파일 직접 읽기, 라이브러리의 환경 변수 읽기 등), 구조(실행 파일 하나, `error.rs`), 크기(파일 · 함수 줄 수) |
| 7 | 비밀 글자 | 키처럼 생긴 글자나 `.env` · `auth.json`이 저장소에 있다 |
| 8 | 바깥 주소 | 테스트가 실제 증권사 · AI 주소를 쓴다 |
| 9 | 검사 끄기 | `#[ignore]`나 지시서에 없는 `#[allow]`가 있다. 화면 · 시험 TS(`.ts` · `.js` · `.mjs` · `.html`)에 `eslint-disable` · `@ts-ignore` · `@ts-nocheck` · `@ts-expect-error` 주석이나 `.skip` · `.only` · `.fixme`가 있다(결정 23) |
| 10 | 공급망 (`cargo deny --offline check licenses bans sources`, 설정은 루트 `deny.toml`) | 허용 목록 밖 라이선스, crates.io 밖 출처(git · 다른 레지스트리), 금지 크레이트(`openssl-sys` · `native-tls`)가 의존 나무에 있다. cargo-deny가 없으면 ✗와 설치 안내(`cargo install cargo-deny --version <scripts/lib.sh DENY_VERSION> --locked`) |
| 11 | 설치 스크립트 시험 (`scripts/test-install.sh`, 전체 범위에서만 — 모듈 범위 실행에서는 돌지 않는다, 결정 27) | 루트 `install.sh`가 가짜 릴리스 묶음(임시 폴더 · 네트워크 없음)으로 깔리지 않거나, sha256이 틀리거나 없는 묶음 · http 주소 · 지원하지 않는 OS/CPU를 거절하지 않거나, `--dry-run`이 무언가 쓰거나, `--uninstall`이 실행 파일 밖의 것(보관 폴더)을 지우거나, 홈(셸 설정 파일)에 무언가 쓰거나, 받는 도구(curl · wget · git)를 불렀다. shellcheck가 있으면 세 스크립트(`install.sh` · `scripts/release.sh` · `scripts/test-install.sh`)도 본다 |

기계가 볼 수 있는 것은 모두 여기에 넣는다. AI 검토는 그 뒤에 남는 것만 판단한다.

**세 범위**: `scripts/verify.sh`(= `--full`)는 전체, `scripts/verify.sh <모듈>…`은 그 모듈만, `scripts/verify.sh --affected [--base <ref>] [--focus a,b] [--wip c,d]`는 영향 범위다. G2(`pipeline.py gate G2`)와 push 전 관문은 영향 범위로 돈다. **릴리스 앞에는 `scripts/verify.sh --full --no-cache`를 한 번 돈다** — `scripts/release.sh`가 빌드 전에 먼저 부른다(실패하면 묶지 않는다). 끝났다는 기준(verify 0)은 그대로다.

**영향 범위** (QMIT 결정 6 · 2026-11-28 사용자 "수정 파일이 영향 있는 곳만 테스트 돌려야지 … 리스크 있는 곳들만 실제 영향이 있는 부분만"). `scripts/affected.py`가 기준(기본 `origin/main`, 없으면 `HEAD~1`; G2는 그 차수가 열린 커밋 — reopen 때 또는 처음 N1을 시작한 때)과 작업 트리(커밋 안 된 것 포함)의 차이로 고른다. 줄이는 것은 "영향이 없음이 확실한 시험"뿐이고, 건너뛴 것은 한 줄씩 `건너뜀: <검사> — 까닭(…)`으로 찍는다.

| 바뀐 것 | 도는 시험 |
|---|---|
| `crates/<X>/`의 코드(`src` · `Cargo.toml` · `build.rs` · `web/` …) | X + X에 (전이적으로) 기대는 크레이트(`cargo metadata`의 path 의존, 보통 · dev · build) |
| `crates/<X>/tests/`만 | X만(시험은 다른 크레이트가 쓰지 않는다) |
| 다른 크레이트의 파일을 글로 읽는 크레이트(예: gate 시험이 `../store/tests/fixtures`를 연다) | 읽히는 크레이트가 바뀌면(시험만 바뀌어도) 읽는 크레이트도 |
| 돈 · 안전 크레이트(store · rules · approval · exec · paper · gate · secrets — `scripts/affected.py` `RISK`) 자신이나 그것이 기대는 크레이트의 코드 | 그 돈 · 안전 크레이트는 늘 돈다(각 `Cargo.toml`을 따로 읽어 센다 — 영향 계산과 상관없이) |
| `crates/server/web/src/<화면>/**` · `web/**/*.html` | vitest: `vitest related <바뀐 파일>`(import 그래프) + 파일을 직접 읽는 시험(`readFileSync` · `?raw` 등, 시험 파일을 읽어 고른다) + 바뀐 시험. e2e: import 그래프로 그 파일을 쓰는 화면을 찾아 그 화면 주소를 여는 spec만(주소를 글로 갖지 않거나 변수로 여는 spec은 늘 넣는다). 화면 빌드 · dist 대조 |
| 화면 공통(`web/src/ui/**` · `paths.ts` · CSS) · 설정(`package.json` · `package-lock.json` · `vite.config.ts` · `tsconfig.json` · `eslint.config.js` · `playwright.config.mjs`) · `public/` · `dist/` · 시험 도움 파일(`tests/web/*.ts` 중 시험 아닌 것 · `tests/e2e/support/**`) | vitest 전체 · e2e 전체 · 화면 빌드 |
| `tests/web/*.test.ts`만 | 그 시험만 |
| `tests/e2e/*.spec.ts` | 그 spec |
| server의 HTTP 경로(라우터 · 조립 `app*` · `main.rs` · `lib.rs` · `*http*.rs` · `page.rs` · `reach/**` · 시연 서버 `*demo*`) | e2e 전체 |
| 루트 `Cargo.toml` · 판이 바뀐 `Cargo.lock` · `scripts/` · `architecture.toml` · `graph.toml` · `clippy.toml` · `deny.toml` · `.cargo/` · `rust-toolchain` · 모르는 곳 | 전체(`범위: 전체 — <까닭>`) |
| 문서만 | 시험 없음, 정적 검사만 |

늘 모든 모듈에 도는 것(빠르다): 0 자체 시험 지문 · 1 모양 · 4 잠긴 시험 · 5 필수 번호 · 5b 원본 근거 · 6 아키텍처 · 7 비밀 · 8 바깥 주소 · 9 검사 끄기 · 10 공급망 · 화면 형 검사(tsc) · 화면 린트(eslint). clippy는 시험을 도는 크레이트에만 돈다(clippy 결과는 그 크레이트와 기대는 크레이트의 코드로만 정해진다). `--focus <모듈>`(G2)은 그 모듈 폴더의 변경만 씨앗으로 삼는다 — 다른 모듈 폴더의 변경은 그 모듈의 G2와 push 전 검사(focus 없음)가 본다. `--wip`(실행기가 넘김)은 다른 노드가 작업 중인(시험이 아직 red일 수 있는) 모듈을 시험 · 정적 검사에서 빼고 `범위 밖:`으로 밝힌다.

**캐시** (2026-11-28 사용자 "캐싱 처리도 해" — 범위는 줄이지 않고, 같은 입력이면 다시 하지 않는다): 검사 단위마다 입력 지문을 셈한다 — 작업 트리(커밋 안 된 것 포함)를 임시 색인으로 git 트리에 적고, 그 단위가 읽는 경로의 트리 id + 검사 스크립트 · 규칙(`scripts/` · `architecture.toml` · `graph.toml` · `deny.toml`) + 도구 판(`rustc -vV` · `cargo -V` · `rustfmt -V` · `node -v`). Rust 시험 · clippy(크레이트마다): 그 크레이트 · 기대는 내부 크레이트 · 글로 읽는 크레이트의 폴더 + 루트 `Cargo.toml` · `Cargo.lock` · `clippy.toml` · `.cargo` · `rust-toolchain`. 모양: `crates/` 전체. 화면(tsc · eslint · vitest · 빌드): `web/`(커밋된 `dist` 포함) · `tests/web` · `tests/e2e`. e2e: 화면 지문 + server 시험 지문. 설치 시험: `install.sh` · `install.ps1`. 같은 지문으로 **통과한** 기록(`.pipeline/cache/passed.json`, git 무시)이 있으면 돌리지 않고 `캐시: <검사> — 같은 입력으로 <때> 통과`로 찍는다. 실패는 적지 않는다. vitest · e2e는 그때 돈 시험(파일 · spec)이 지금 필요한 것을 다 덮을 때만 캐시로 본다. 돈 · 안전 크레이트도 지문이 같으면 같은 결과라 캐시를 쓰고, 지문이 달라지면 반드시 돈다. 그래서 G2를 통과한 바로 뒤의 push 전 검사는 같은 트리 · 같은 범위를 다시 돌리지 않는다(정적 검사만 다시 돈다). `--no-cache`는 캐시를 보지 않는다(기록은 한다). 지우기: `pipeline.py cache clear`. `npm ci`는 `package-lock.json` 지문(`node_modules/.pi-lock.sha256`)이 같고 `node_modules`가 있으면 건너뛴다. e2e가 쓰는 서버 실행 파일은 cargo 증분 빌드가 맞춘다(바뀐 것이 없으면 확인만). 새 도구(sccache 등)는 들이지 않는다.

GitHub CI는 쓰지 않는다(2026-11-24 사용자 "깃허브 ci는 하지말자 로컬에서 다해결하면돼"): push 전 검사는 pre-push 관문이, 전체 검사는 손으로 돌리는 `scripts/verify.sh`와 릴리스 앞의 `--full --no-cache`가 맡는다.

**로컬 pre-push 관문** (QMIT 결정 4): `.githooks/pre-push`가 push되는 범위(원격 sha 기준, 새 브랜치면 `origin/main`)로 `verify.sh --affected`를 돌리고 실패하면 push를 막는다. 저장소마다 한 번 `git config core.hooksPath .githooks`로 켠다(꺼져 있으면 `verify.sh`가 참고 줄로 알린다). 관문을 건너뛰는 push는 하지 않는다.

**공급망 규칙** (QMIT 결정 8):
- **새 바깥 의존은 사용자 승인으로만.** 새 바깥 의존(루트 `Cargo.toml` `[workspace.dependencies]`에 새 이름, `crates/*/web/package.json`에 새 이름)은 N2(와 다른 노드)가 더하지 않고 멈춰 보고한다. 조정자가 사용자 승인을 받아 `architecture.toml`(그 크레이트의 `external`, 테스트 · 실행 파일 전용이면 `[dep_workspace] extra`, 화면이면 `[web] allowed`)과 의존을 같은 커밋에 더한다. 기계로는 6번 검사가 막는다: `[workspace.dependencies]`의 바깥 이름이 크레이트별 `external`의 합(`price-inha`의 `"*"`는 빼고)과 `[dep_workspace] extra` 밖이면 ARCH-DEP-NEW, `package.json`의 의존 이름이 `[web] allowed` 밖이면 ARCH-WEB-DEP.
- **오프라인(G2)**: 10번 검사(cargo-deny licenses · bans · sources). 라이선스 허용 목록 · 예외는 `deny.toml`에 있고, 바꾸는 것은 조정자가 사용자 확인 뒤에만 한다.
- **네트워크(G2 밖)**: 보안 공지(`cargo audit` · `cargo deny check advisories`, 무시 목록은 `.cargo/audit.toml`과 `deny.toml`에 같은 사유로)와 새 판 발행일(`scripts/supply.py`)은 pre-push 관문과 `scripts/nightly.sh audit`이 본다. pre-push는 기준 판에 없던 새 판만 보고 `[supply] min_age_days`(14일)보다 새것이면 막는다. `nightly.sh audit`은 손으로 돌리며 `--all`로 모든 판을 보고만 한다.
- **발행일 검사** `python3.14 scripts/supply.py [--since <ref>] [--head <ref>] [--all]`: `Cargo.lock`의 crates.io 패키지와 `crates/*/web/package-lock.json`의 npm 패키지의 발행 시각을 crates.io · npm 레지스트리에서 받는다(동시 4개 이하, 캐시 `.pipeline/supply-cache.json`). 기본 기준은 `origin/main`. 종료 0 통과 · 1 너무 새 판 · 2 기준 오류 · 3 발행일을 받지 못함(네트워크). `--offline-fixture <json>`은 자체 시험용 가짜 발행일 표다.
- **pre-push**: push 범위에서 `Cargo.lock`이나 `package-lock.json`이 바뀌었을 때만 `supply.py --since <원격 sha> --head <로컬 sha>`와 `cargo deny check advisories`를 돌린다. 너무 새 판이거나 보안 공지가 있으면 push를 막고, 네트워크가 없으면 경고만 한다.

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

새 모듈(template 아래 12개)은 2026-11-16 로드맵(T6~T22, 결정 문서 2026-11-16 절 · 옛 `docs/dev/roadmap.md`는 다 만들어 지웠다 — git 기록)으로 더했다. 표의 행은 계획이고, `architecture.toml`의 `[crates.*]` 항목은 그 크레이트를 처음 만드는 N1 커밋과 함께 더한다(없는 크레이트가 표에 있으면 ARCH-CRATE가 모든 모듈 검사를 깨기 때문이다).
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
| `lock <모듈>` | 테스트를 커밋하고 `red/<모듈>` 태그로 잠근다. `gate G1`이 통과하면 실행기가 바로 부른다. 잠그기 직전에 red를 한 번 더 확인하되, G1이 같은 트리(지문)를 방금 확인했으면 다시 돌리지 않는다. 바뀐 시험이 이미 커밋돼 있으면(동작 잠금 등) 빈 커밋 없이 태그만 옮긴다(작업 트리의 시험이 HEAD와 같아야 한다) |
| `gate-batch <GR\|G1\|G2> <모듈…>` | 여러 모듈(합친 뒤의 동작 잠금 여럿 등)의 관문을 한 번에. GR은 모듈마다, G1은 시험 빌드를 한 번 하고 모듈마다 `red.sh`(통과하면 잠금), G2는 묶음의 영향 범위로 `verify.sh`를 **한 번** 돌리고 결과를 모듈마다 나눠(모듈 이름이 적힌 ✗는 그 모듈, 모양 · clippy 같은 공통 ✗는 그 출력에 적힌 `crates/<모듈>/`, 없으면 묶음 전부) 기록 · 커밋한다 |
| `review <모듈> <통과\|되돌림>` | AI 검토 판정. 되돌림이면 N2로 돌아가고, 2번을 넘으면 사람에게 넘긴다. 통과면 그 모듈 지시서에 `tidy`를 돌린다(11장) |
| `tidy [--all] [--dry-run] [<모듈…>]` | 문서 정리(11장) |
| `flaky` | 흔들린 시험(혼자 다시 돌려 통과한 것)과 횟수 — 고칠 후보 |
| `cache clear` | 검사 캐시(`.pipeline/cache`)를 지운다 |
| `scope` | 진행 중인 노드가 맡지 않은 곳을 고쳤는지 |
| `resume <모듈>` | 사람에게 넘어간 모듈을 사람이 다시 연다 |
| `reopen <모듈> [--lock] <이유>` | 조정자가 모듈을 다시 연다(새 차수 · 테스트 결함). 잠금 태그를 지우고, 그 판을 `base`로 남기고, R(대조)부터 돌게 한다. `--lock`은 동작 잠금 모드 |
| `amend <모듈> <이유>` | 뜻이 안 바뀌는 테스트 결함(이름 · 모양)을 조정자가 바로잡고 다시 잠근다 |

## 9. 검사기 자체 시험 (`scripts/selftest.py`)

검사기가 위반을 실제로 잡는지 확인한다. 깨끗한 가짜 저장소를 만들어 규칙을 하나씩 어기고, 그 규칙 이름으로 잡히는지 본다. 깨끗할 때 통과하는지도 본다.

- 규칙(`architecture.toml`), 검사 스크립트, 그래프, 루트 `Cargo.toml` · `clippy.toml`을 고치면 자체 시험을 다시 돌려야 한다. `verify.sh`의 0번 검사가 규칙의 지문이 마지막 자체 시험 때와 같은지 본다.
- `architecture.toml`에 금지 모양을 더하면 `selftest.py`의 표본에도 "어긴 코드 한 줄"을 더해야 한다. 없으면 자체 시험이 실패한다.

## 10. 로컬 측정: 변이 시험 · 커버리지 · 반복 실행 — 보고만, 판정 아님

GitHub CI는 쓰지 않는다(2026-11-24 사용자). 게이트 자체를 재는 측정(QMIT 결정 7)은 손으로 `scripts/nightly.sh`를 돌린다. 결과는 `target/nightly/`에 남고 **성공 · 실패 판정에는 넣지 않는다**. 끝났다는 기준은 여전히 `scripts/verify.sh`뿐이다.

| 명령 | 하는 일 |
|---|---|
| `scripts/nightly.sh audit` | 보안 공지(`cargo audit` · `cargo deny check advisories`)와 모든 판 발행일 보고(`supply.py --all`). 네트워크 필요 |
| `scripts/nightly.sh mutants [-p 패키지…]` | `cargo-mutants`로 돈 · 안전 핵심 크레이트(기본 `pi-rules` · `pi-paper` · `pi-exec` · `pi-gate`)의 시험이 일부러 넣은 버그를 잡는지(caught · missed) |
| `scripts/nightly.sh coverage` | `cargo-llvm-cov`로 워크스페이스 줄 커버리지를 크레이트별 표로 |
| `scripts/nightly.sh stress <번> [cargo test 인자…]` | 간헐 실패 찾기: 여러 번 돌려 실패 줄만 모은다 |
| `scripts/nightly.sh e2e <번>` | 브라우저 e2e를 여러 번 돌려 흔들리는지 · 한 번에 걸린 시간을 본다(결정 23) |
| `scripts/nightly.sh all` | audit · mutants · coverage 차례로 |

놓친 변이가 보이면 해당 모듈을 다시 열 때(reopen) 시험을 보강하는 근거로 쓴다. 도구 판은 발행 14일 넘은 것만 고정한다(공급망 규칙 8 · 9): cargo-mutants 27.1.0, cargo-llvm-cov 0.9.1. 도구가 없으면 스크립트가 설치 명령을 알려 준다.

## 11. 문서 정리 (`pipeline.py tidy`)

2026-11-28 사용자 "다 구현하면 코드베이스로 알 수 있는 문서들은 다 제거해" · "하네스에도 문서 정리하는 거 만들어서 진행". 지난 일의 자세한 글은 git 기록이 맡고, 문서에는 지금 일에 필요한 것만 둔다.

- **지시서** `docs/dev/modules/<모듈>.md`: 차수 절(`## <N>차 …`) 가운데 마지막 것(지금 차수)을 빼고, 옛 차수 절의 본문을 한 줄로 줄인다 — 머리 줄은 남기고 본문 대신 `- 끝남: <본문 첫 줄> — 커밋 <red 해시>(red) · <green 해시>(green) · 전문: git show <정리 전 해시>:<지시서>`. 그 차수의 `### 대조` 표도 함께 빠진다(git 기록). 차수 절이 아닌 절(원본 동작 근거 · 만드는 것 · 필수 번호 · 하지 않는 것 · 테스트 주의 · 다음 차수 후보 …)은 한 글자도 바꾸지 않는다 — 바뀌려 하면 tidy가 멈춘다(5 · 5b 검사가 읽는 절이 그대로다).
- **언제**: 차수 사이에만 — 모듈이 `검토 통과`이거나 아직 연 적이 없을 때(`할 일`이고 대조 먼저가 아닐 때). 차수가 진행 중이면 `건너뜀: tidy <모듈> — 까닭(차수 진행 중 …)`. `review <모듈> 통과` 뒤 그 모듈에 자동으로 돌고, 바뀌면 `docs(<모듈>): 지시서 정리`로 커밋한다. 손으로는 `pipeline.py tidy [--all] [--dry-run] [<모듈…>]`(바뀐 것을 `docs: 문서 정리` 한 커밋으로).
- **GR과의 관계**: GR은 "다시 연 때의 지시서보다 대조 표가 하나 늘었다"를 센다. 다시 연 때의 표 가운데 지금 지시서에서 그 절이 `- 끝남:` 한 줄로 줄여진 것은 옛 수에서 뺀다 — tidy 뒤(또는 reopen 뒤에 줄인 지시서가 합쳐져도) 새 표 하나로 통과하고, 옛 표를 고쳐 쓰기만 하면 여전히 실패한다.
- **`--all`이 더 하는 것**: `docs/dev/backlog.md`의 끝난 줄(`- [x]`과 그 밑줄)과 "진행 기록" 절을 지우고, 남은 `- [ ]`이 없는 절도 지운다. 머리 줄 말고 글이 없는 빈 문서(`docs/**/*.md`)를 지운다(스크립트가 읽는 것은 둔다).
- **지우지 않는 것**: 설계서(`docs/product/`) · 결정(`decisions.md`) · 조사(`docs/research/`, 빈 것 말고) · `conformance.md` · `toss-api-notes.md` · `keys-needed.md` · `architecture.toml` · 이 문서 · 프롬프트 틀.
