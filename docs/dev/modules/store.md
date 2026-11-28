# store 모듈 (T1)

크레이트 `pi-store`, 폴더 `crates/store/`. 구성 요소 A12 저장소.
근거: `dev.md`의 D6 · M1 · M2 · M6(기록만) · C2 · E3, `spec.md`의 상태표(투자 팀 · 매매).

## 원본 동작 근거

- 대조표: `docs/dev/conformance.md` 3장 "저장소".
- 따르는 원본: Hermes `state.db`의 저장 방식 — `docs/research/hermes-agent.md` 3.5 · 7.1. 이벤트 로그와 다시 잇기 — `docs/research/deskrpg-hermes-plugin.md` 3-4.
- 의도해서 다른 것: SQLite 파일 하나(D6), 더 새 버전의 파일은 열지 않는다(원본은 연다 — 우리 쪽이 더 안전해서).
- 모르고 달랐던 것(다음 차례에 고친다): 이벤트 종류 이름 규칙, 이벤트 시각 칸, DB 파일 권한 0600, 이벤트 행의 방 · 멱등 키 · 행위자 칸.

## 만드는 것

1. **열기와 이전**: SQLite 파일 하나를 연다. 스키마 버전을 두고 순서대로 올린다. 다시 열면 자료가 그대로다. WAL과 외래 키를 켠다.
2. **투자 팀 상태 (M1)**: 상태 5개(운용 중, 쉬는 중, 하루 한도 멈춤, 이상 감지 멈춤, 전체 정지). 처음은 쉬는 중.
   - 전체 정지에서 나가는 것은 "내 승인" 표시가 있을 때만 된다.
   - 하루 한도 멈춤은 "다음 거래일 시작" 사유로만 풀린다.
   - 바뀔 때마다 언제 · 무엇에서 무엇으로 · 왜가 기록으로 남는다. 기록은 지우거나 고치는 API가 없다.
   - 허용되는 전이는 `spec.md` 상태표를 따른다. 표에 없는 전이는 오류다.
3. **매매 (M2)**: 상태 8개(논의중, 판단됨, 승인대기, 막힘, 주문됨, 보유중, 팔림, 복기됨). 전이는 `spec.md` 상태표를 따른다.
   - 근거(가설 · 계기 · 들어간 값 · 손절값 · 목표값 · 기대값)가 하나라도 비면 주문됨으로 갈 수 없다.
   - 판단에 쓴 자료의 "알게 된 시각"이 판단 시각보다 늦으면 저장을 거부한다.
   - 보유중일 때 손절값은 좁히는 쪽(사는 매매 기준 더 높게)으로만 바꿀 수 있다.
   - 계좌 종류(모의 · 실전), 담당 점수와 합의 점수, 승인 번호와 내용 지문을 함께 둔다.
   - 금액과 가격은 부동소수점으로 두지 않는다(정수 최소 단위나 십진수 문자열).
4. **이벤트 기록 (C2 · E3)**: `append(종류, 내용 JSON) → 번호`. 번호는 1부터 빠짐없이 커진다. `read_after(번호, 최대 개수)`로 그 뒤의 이벤트를 순서대로 준다.
5. **접속 기록 (M6)**: 언제 · 어디서 · 무엇을 했는지 쌓는다. 지우거나 고치는 API가 없다.
6. **비밀 없음**: 키 값을 받는 칸이나 API를 만들지 않는다.

## 2차 작업 (2026-11-14 사용자 결정 + 1차 AI 검토)
- 끝남: 1차(위 1~6번)는 구현 · 검토가 끝났다. 2차는 아래를 테스트로 먼저 쓰고(바뀌는 기존 테스트 포함) 구현한다. 1차의 다른 동작과 공개 API 모양은 그… — 커밋 e66560b(red) · 9a7610b(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 3차 작업 (원본 대조에서 "모르고 다름"으로 나온 것)
- 끝남: 근거: docs/dev/conformance.md 3장의 C4 · C8 · C9, 원본은 docs/research/hermes-agent.md 6.5 · 7.1… — 커밋 fbf27d3(red) · 961937a(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 4차 작업 (3차 AI 검토의 참고 + 옛 기록 경계 번호)
- 끝남: 4차는 아래 1~5를 테스트로 먼저 쓰고 구현한다. 기존 공개 API의 모양은 바꾸지 않고 더하기만 한다. — 커밋 b0f601e(red) · c7dedd2(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 5차 작업 (4차 보완 검토의 참고 — 2026-11-15 사용자 "다 개선")
- 끝남: 공개 API는 바꾸지 않는다. 접속 관문 쪽은 gate 7차 6번에서 같은 규칙으로 맞춘다. — 커밋 e069244(red) · ad0747f(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 6차 작업 (5차 검토의 참고)
- 끝남: 아래를 테스트로 먼저 쓰고 구현한다. 공개 API는 바꾸지 않는다. — 커밋 f69f344(red) · 76adb20(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 7차 작업 (gate 7차 6번을 받친다 — 2026-11-15)
- 끝남: 접속 관문은 이벤트 내용을 이미 읽힌 JSON 값으로만 받아서, 맨 위 칸 이름이 겹친 줄을 옛 봉투와 가를 수 없다. 옛 봉투 판정은 원래 글자를 아는 저장소… — 커밋 5b866d3(red) · 715d8f0(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 8차 작업 (7차 검토 · gate 7차 검토의 참고 — 2026-11-15 조정자 결정)
- 끝남: 칸 이름에 이스케이프가 든 줄(예: {"\u0061t":1,"payload":2}, {"a\u0074":…})도 옛 봉투 판정이 접속 관문의 serde_json… — 커밋 d59dfd5(red) · b72a447(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 9차 작업: 템플릿 · 승인 · 개선안 · 모의계좌 스냅샷 · 접속 키 표 (로드맵 W1, 2026-11-16 결정)
- 끝남: 근거: dev.md E36 · M3 · M4 · M5 · M6 · V6 · V29 · V30, 로드맵 W1(옛 docs/dev/roadmap.md, git 기록… — 커밋 26bdb12(red) · 766f3f4(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 10차 작업: 에이전트 세션 · 메시지 · 방 로그 · 트리거 영수증 · 자료 (로드맵 W2)
- 끝남: 근거: dev.md C11 · V36 · M2, 원본 근거 docs/research/agent-runtime-deep.md 1 · 7장, docs/researc… — 커밋 26bdb12(red) · 766f3f4(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 11차 작업: 지식 · 스냅샷 · 되살리기 표시 · 웹 푸시 구독 (로드맵 W3)
- 끝남: 근거: dev.md D10 · M7 · V51, 원본 근거 docs/research/triggers-approval-memory-deep.md 4.1 · 6.1. — 커밋 26bdb12(red) · 766f3f4(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 9차 보강 (9차 검토 되돌림 — 2026-11-16, 사용자가 다시 엶)
- 끝남: 표가 스스로 지켜야 한다(어떤 SQL로도): — 커밋 7520b2e(red) · 4f89552(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 10차 보강 (10차 검토 되돌림 — 2026-11-16)
- 끝남: SQLite는 recursive_triggers가 꺼져 있어 INSERT OR REPLACE · REPLACE INTO가 충돌한 줄을 지울 때 지우기 막기 트리… — 커밋 c84d00d(red) · d018793(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 10차 보강 2 (보강 검토 되돌림)
- 끝남: rowid를 명시한 INSERT OR REPLACE (rowid, …)는 선언 키만 보는 트리거를 비켜 옛 줄을 지운다. INTEGER PRIMARY KEY 별… — 커밋 771b108(red) · 3380b7b(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 12차 작업 (후보 일괄 처리, 2026-11-16 사용자 "다 처리해")
- 끝남: "다음 차수 후보" · "11차 검토 참고" · "다른 모듈에서 넘어온 것"을 모두 처리했다. 공개 API는 더하기만 했다(다른 모듈이 쓴다). 스키마는 8로 … — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 다음 차수 후보

(12차에서 모두 처리했다.)

## 모양

- `Store`는 `Clone + Send + Sync`이고 동기 API다(호출하는 쪽이 필요하면 `spawn_blocking`).
- 오류는 `thiserror`로 만든 `StoreError` 하나. 상태 전이 거부는 따로 구분되는 종류다.
- 시각은 호출하는 쪽이 넘긴다(테스트가 시계를 정할 수 있게). 내부에서 `now()`를 부르지 않는다.
- SQLite는 번들 빌드(`rusqlite`의 `bundled`)를 쓴다.

## 하지 않는 것

템플릿(M3) · 규칙 개선안(M4) · 지식 검색(M7) · 승인 요청 전체(M5) · 접속 키 해시와 잠금(T17)은 나중 작업이다.

## 필수 번호

M1 M2 M5 C2 E3 M6 V6 D6 E12 E36 M3 M4 V29 V30 C11 V36 D10 M7 V51

### 11차 검토 참고 (12차에서 처리함)

- `knowledge_chunks_no_replace`는 `id` 충돌만 막는다. `UNIQUE(kind, record_ref, ordinal)`에 걸리는 `INSERT OR REPLACE`는 옛 줄을 트리거 없이 지워 FTS 줄과 임베딩이 고아로 남는다(store 코드는 이 SQL을 쓰지 않음). 트리거에 같은 (kind, record_ref, ordinal) 검사를 더한다.
- `snapshot_to`는 백업 동안 연결 잠금을 쥔다. 파일이 크면 쓰기가 오래 기다린다.
- 매매 방향(사기/팔기)을 저장한다(agent 2차 검토): 지금은 팔기 판단이 agent 메모리에만 있어 재시작하면 사라진다(rules · 승인이 막아 안전하지만 기록이 없다).

### 후보 일괄 처리 때 다른 모듈에서 넘어온 것 (12차에서 처리함)
- 판단에 방향(사기 · 팔기) 칸: 서버를 다시 띄운 뒤에도 agent가 팔기 매매의 `draft_order`를 막을 수 있게 한다.
- 매매 상태 바뀜 이력 조회 API(journal 하루 정리가 중간 바뀜을 본다).
- 개선안 위험 표시 갱신 API(journal `approve_as_risky` 우회를 없앤다).
- 지식 조각 번호 조회 · 메타 키 지우기 API(knowledge가 조각과 메타를 한 트랜잭션으로 묶고 빈 값 대신 키를 지운다).
- Trade에 수량 칸(exec가 사는 수량을 승인 내용과 대조한다).

## 13차 작업 (12차 검토 참고)
- 끝남: knowledge_chunks에 BEFORE UPDATE 보호: UPDATE OR REPLACE ... SET ordinal처럼 다른 줄과 겹치게 바꿔 지우기 … — 커밋 3adf588(red) · 7312792(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 14차 작업 (후보 일괄 처리)
- 끝남: Store::room_last_seq(room: &str) -> Result<u64, StoreError>: 방의 마지막 번호 한 줄 읽기. 이벤트가 없거나 없… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 15차 작업 (2026-11-24 조정자, server 21차 막힘 — 결정 9 · 14)
- 끝남: server 21차 1번(실전중 템플릿의 위험을 키우는 사용자 변경은 승인 → 냉각 → 다음 거래일 반영)과 6번(고른 외형이 백업에 들어감)을 저장소가 받친다. — 커밋 47d00a4(red) · 1b8526b(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 16차 작업 (2026-11-24 조정자, 15차 뒤 자동 보안 검토 — 승격 검증 우회 막기)
- 끝남: 15차의 사용자 변경 반영이 승격대기 템플릿에도 새 리비전을 쓸 수 있어, 기준 통과 기록(pass_revision)과 다른 값이 그대로 실전으로 승격될 수 있… — 커밋 d595327(red) · aa23ebe(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 17차 작업 (2026-11-24 조정자, 15차 검토 참고 — 사용자 "남은 일 한 개도 없이")
- 끝남: 새 기록 표의 지우기 막기: 판 10의 proposal_sources · user_revisions에 BEFORE DELETE 트리거를 더한다(다른 기록 표와 … — 커밋 144548e(red) · 4d1a524(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 18차 작업 (2026-11-25 조정자, 결정 17 — 히스토리 화면의 조회)
- 끝남: 근거: docs/product/stock-team/additions.md의 C30 · T38(차수 표 6번), 스펙 R6-3 · R4-3 · F7-7. serv… — 커밋 d4ff2b0(red) · 34086ee(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 19차 작업 (2026-11-25 조정자, 결정 17 — 판 11: 내 캐릭터 · 담당 의견 · 공지 · 결정 메모)
- 끝남: 근거: docs/product/stock-team/additions.md의 M8 · M9 · M10 · M11 · D13 · T23(차수 표 10번). 판 10… — 커밋 942687c(red) · 3818563(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 20차 작업 (2026-11-26 조정자, 결정 25 — 사람이 내리는 전체 정지)
- 끝남: 근거: 결정 25, server 39차 N1 보고("전체 정지로 가는 전이를 최대 손실 사유로만 받는다 · 쉬는 중 → 전체 정지 전이가 없다"), 스펙 F4(… — 커밋 c1ef2fd(red) · c8ce8cb(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 21차 작업 (2026-11-27 조정자, 결정 31 — 사용자 "경량화 작업도 해야 해" · "노이즈는 최대한 제거하고 가져갈 규칙 기억들만 쌓는다")
- 끝남: 동작 잠금 차수다. 별도 작업 폴더(기능 M, 결정 28 방식)에서 red → green으로 만든 판 12를 main에 합쳤고, 이 차수는 합친 구현에서 시험이… — 커밋 510a790(red) · a7fb9d0(green) · 전문: `git show 1a73576:docs/dev/modules/store.md`

## 22차 작업 (2026-11-27 조정자, 결정 32 · 백로그 R · P — 사용자 "백로그 다 실행해" · "남은 것들만 다 실행해 key 제외하고")
- 끝남: 동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지… — 커밋 ff3c500(red) · 0383887(green) · 전문: `git show aa0b083:docs/dev/modules/store.md`

## 23차 작업 (2026-11-28 조정자, 백로그 U — 사용자 "다 처리해")
- 끝남: 동작 잠금 차수다. 별도 작업 폴더(결정 28 방식)에서 red → green으로 만든 것을 main에 합쳤고, 이 차수는 합친 구현에서 시험이 모두 통과하는지… — 커밋 265727f(red) · —(green) · 전문: `git show ac8af3e:docs/dev/modules/store.md`

## 24차 작업 (2026-11-28 조정자, 사용자 "다 되면 임시 파일들이나 불필요한 것들 제거해")
- 끝남: 정리 차수다. 시험 폴더에 컴파일되지 않는 옛 백업 파일 tests/round18.rs.bak이 커밋돼 남아 있다. 지운다. 동작 · 시험은 바뀌지 않는다. — 커밋 —(red) · —(green) · 전문: `git show ac8af3e:docs/dev/modules/store.md`

## 24차 보강 (2026-11-28 조정자, 사용자 "다 되면 임시 파일들이나 불필요한 것들 제거해")

정리 차수다. 시험 폴더에 컴파일되지 않는 옛 백업 파일 `tests/round18.rs.bak`이 커밋돼 남아 있다. 지운다. 동작 · 시험은 바뀌지 않는다.

- 원본 동작 근거: 해당 없음(정리).
- 시험으로 확인할 것: 지운 뒤에도 store 시험이 모두 통과한다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| `tests/round18.rs.bak` | 지움 | 사용자 "다 되면 임시 파일들이나 불필요한 것들 제거해" — `.rs`가 아니라 cargo가 읽지 않는 백업 파일(시험 수 그대로). 같은 내용의 `tests/round18.rs`는 그대로 |
| 그 밖의 store 시험 | 유지 | 바뀌지 않는다 |
