# approval 모듈 (T14)

크레이트 `pi-approval`, 폴더 `crates/approval/`. 구성 요소 A7. `pi-store`(9차의 승인 표)를 쓴다. 시각은 Clock으로 받는다.
근거: `dev.md` C7 · C12 · M5 · E7 · E8 · E22 · N2 · V42~V45, `spec.md` FN2, `principles.md` P18~P20.

## 원본 동작 근거

- 상태 기계와 사유(기다림 → 승인 · 거절 · 답 없음 · 만료, 한 번만 바뀐다. 답 없음 · 모르는 답은 거절), 지문을 실행 직전에 다시 비교, 한 번만 소비(`consumed_at`): `docs/research/triggers-approval-memory-deep.md` 3.2 · 3.4, `docs/research/openclaw.md` 2.2.
- 화면에 띄우고 답을 돌려보내기, 같은 요청 합치기: `docs/research/deskrpg.md` 4-7, `docs/research/deskrpg-hermes-plugin.md` 4-1 · 3-4, `docs/research/hermes-agent.md` 5.2.
- 위험을 줄이는 쪽은 바로: `docs/research/remote-access-security.md` 5.3.
- 의도해서 다름: 답 없음 처리(FN2 3b — 원본은 거절 하나), 다시 켠 뒤 신호 유효 시간 안의 승인 유지(원본은 전부 닫음), 승인 대상에 실전 승격 · 규칙 개선 · 다시 시작을 더함.

## 만드는 것 (1차)

1. `Subject`: 주문(계좌 종류 · 종목 · 방향 · 수량 · 가격 · 주문 종류 · 신호 id), 실전 승격(템플릿), 규칙 개선(개선안), 다시 시작. 승인이 필요한 주문은 지정가로만 내고 가격을 지문에 넣는다(결정 7번).
2. `fingerprint(subject)`: 칸 순서를 정한 정규 JSON의 SHA-256. `short_code`는 지문 앞 6자리이고 카드에 보인다. 사용자가 숫자를 치지는 않는다(결정 4번).
3. `request(C12)`: 승인 번호(무작위, 다시 쓰지 않음)를 만든다. 기다릴 시간은 신호 종류별 템플릿 값(N2)이다. 같은 대상 · 같은 지문이 기다리는 중이면 새로 만들지 않고 횟수만 올린다.
4. `answer(C7: 번호, 화면 지문, 승인|거절)`
   - 번호가 없거나 · 이미 처리됐거나 · 만료면 거절로 남긴다(E8 · V45).
   - 지문이 다르면 거절(E7).
   - 거절은 바로 된다(V43).
5. `consume(번호, 실행할 지문)`: 승인됨이고 아직 소비되지 않았고 지문이 같을 때만 한 번 통과한다(V44). 서로 다르면 거절로 바꾼다.
6. `expire_due(now)`: 답 없음으로 바꾸고 처리 방침을 돌려준다(E22).
   - 사는 주문: 알림이 갔으면 금액 한도 안으로 줄여 다시 점검(FN2 3b3). 알림 보내기에 실패했으면 사지 않는다(P20, 결정 5번).
   - 파는 신호: 팀이 판단.
   - 다시 시작 · 규칙 개선 · 실전 승격: 계속 기다린다(V42).
7. 다시 켰을 때 신호 유효 시간이 지난 것은 만료하고, 안이면 유지한다. 백업에서 되살릴 때는 store가 전부 만료한다(M5, 결정 6번).

## 저장소와 맞출 것 (store 9차 보강 검토의 참고)

- 승인 요청에 대상 종류(주문 · 실전 승격 · 규칙 개선 · 다시 시작)와 대상 번호를 지문과 함께 남겨, 한 대상을 위해 받은 승인을 다른 대상에 쓸 수 없게 한다(지금 store는 템플릿 · 개선안끼리만 막고, 주문용으로 소비된 승인은 범위 밖). 필요하면 store 다음 차수에 승인 대상 칸과 트리거를 더한다.
- 전체 정지 중 실전 승격을 막을지 정한다(추천: 막는다 — 전체 정지는 재개를 사람만 한다).

## 2차 작업 (1차 검토 되돌림)

1. 실전 승격 · 규칙 개선은 이 크레이트로만 끝까지 간다: `promote_live(번호, 템플릿 번호)` · `approve_proposal(번호, 개선안 번호)`가 무효 번호 · 전체 정지를 확인하고, 대상에서 지문을 계산해 store의 `promote_template_live` · `approve_proposal`을 부른다. 실전 승격 지문에는 템플릿 판(revision)도 넣는다.
2. 실행 때 지문이 달라 무효가 된 승인은 `get()`에서 `Rejected`(까닭: 내용 바뀜)로 보인다. 다시 켜도 같다.
3. 기다릴 시간(`wait_until`)이 지난 뒤 온 답은 만료로 닫는다(원본 3.2-2). 기한이 지난 기다림에는 합치지 않는다.

## 4차 작업 (2026-11-21 발표 자료 대조)

근거: `spec.md` FN2 3b(답 없음) · F6 5~8(승격), `dev.md` E22("보이는 것: 답이 없어 팀이 어떻게 처리했는지 기록이 보인다") · E36 · V42, `docs/dev/decisions.md` 2026-11-16 5 · 6번, 2026-11-21 6번(승격은 상태 · 승인까지 서버에 잇는다).
지금 `expire_due`(`src/due.rs:18-53`)는 답 없음으로 닫고 할 일(`DueAction`)을 한 번만 돌려준다. 그 뒤 무엇을 했는지 남길 곳이 없고, 돌려받은 뒤 처리 전에 프로그램이 꺼지면 할 일을 잃는다. server는 아직 `expire_due` · `promote_live`를 부르지 않는다. 이 차수는 **모듈 쪽에 빠진 공개 API**만 더한다. 줄인 수량 계산은 rules 4차 4번(`reduce_within_limit`), 부르는 고리(주기적으로 `expire_due` → 할 일 → 기록, 승격 `evaluate_promotion` → 승격대기 → `request` → `promote_live`)는 server 차수다.

### (a) 공개 API 바뀜
- `pub enum DueHandling { Reduced { quantity: u64 }, NotBought, TeamSold, TeamKept, Blocked { reason: String }, Dropped }`. 뜻: 줄여서 냄(줄인 수량) · 사지 않음(P20) · 팀이 팔기로 판단 · 팀이 들고 있기로 판단 · 다시 점검에서 막힘(막힌 까닭 글, 가린 것) · 신호가 지나 아무것도 안 함.
- `pub fn Approvals::record_due_handling(&self, number: &ApprovalNumber, handling: DueHandling) -> Result<(), Error>`.
- `pub fn Approvals::due_handling(&self, number: &ApprovalNumber) -> Result<Option<DueHandling>, Error>`.
- `pub fn Approvals::unhandled_due(&self) -> Result<Vec<DueOutcome>, Error>`.
- `Error`(`non_exhaustive`)에 `NotDue`(답 없음 · 만료로 닫힌 주문 요청이 아님) · `AlreadyHandled`를 더한다.
- `DueOutcome` · `DueAction` · `ApprovalView`의 모양은 바꾸지 않는다.

### (b) 동작 규칙 · 경계
1. `record_due_handling`
   - 그 번호가 주문 대상이고 결과가 답 없음 또는 만료일 때만 받는다. 아니면 `NotDue`(번호가 없어도 `NotDue`).
   - 한 번만 남는다. 두 번째는 `AlreadyHandled`이고 처음 기록이 그대로다.
   - 할 일과 맞지 않는 기록은 거부한다(`NotDue`): `ReduceWithinLimit` ← `Reduced` · `Blocked` · `Dropped`, `DoNotBuy` ← `NotBought`, `TeamDecides` ← `TeamSold` · `TeamKept` · `Blocked` · `Dropped`, `Dropped` ← `Dropped`. 할 일은 `expire_due`와 같은 규칙으로 다시 센다.
   - `Reduced { quantity }`는 0보다 크고 승인 요청의 수량보다 작거나 같아야 한다(아니면 `NotDue`). 줄이는 셈은 rules가 했다.
   - 저장은 지금 알림 결과(`record_notice`)와 같이 저장소 이벤트로 남기고, 다시 켠 뒤에도 메모(`record.rs`의 `refresh`)로 읽힌다. 글(`Blocked`의 까닭)은 남기기 전에 가린 것을 받는다(가리는 일은 부르는 쪽, E38).
2. `unhandled_due`
   - 답 없음 · 만료로 닫힌 주문 요청 가운데 처리 기록이 없는 것을 닫힌 순서로 돌려준다. 할 일은 `expire_due`와 같은 규칙이다. 다만 지금 시각(시계)이 신호 유효 시간의 끝 이후면 `Dropped`로 돌려준다(꺼졌다 켜진 사이 신호가 지나면 줄여서 사거나 팀이 팔지 않는다 — 결정 6번과 같은 생각).
   - 읽기만 한다(상태를 바꾸지 않는다). server는 켤 때와 `expire_due` 뒤에 이것을 보고 남은 할 일을 마저 한다.
3. 답 없음 처리에서 사는 주문을 줄여 낼 때는 새 승인 요청을 만들지 않는다(줄인 주문은 금액 한도 안이라 승인이 필요 없다). 답 없음으로 닫힌 요청은 지금처럼 `consume`으로 쓸 수 없다.
4. 다시 시작 · 규칙 개선 · 실전 승격은 지금처럼 계속 기다린다(V42). 이 셋에는 `record_due_handling`이 `NotDue`다.
5. 승격: 모듈 쪽 API(`request(Subject::LivePromotion { .. })` · `promote_live`)는 이미 있다. 더할 것이 없다. 전체 정지 중 승격을 막는 것도 2차 · store 12차 그대로.

### (c) 원본 동작 근거
- 한 번만 바뀌는 상태 · 한 번만 소비 · 다시 켠 뒤 정리: `docs/research/triggers-approval-memory-deep.md` 3.2 · 3.4, `docs/research/openclaw.md` 2.2.
- 답 없음의 처리 갈래는 원본에 없음(원본은 거절 하나) — 의도해서 다름(1차 그대로), 스펙 FN2 3b · `dev.md` E22. 승인 경로가 막혔을 때의 보수적 처리: `docs/research/remote-access-security.md` 5.4.
- 처리 기록을 남기는 것은 원본에 없음 — `dev.md` E22 "보이는 것"을 지키려고 더한다.

### (d) 테스트로 확인할 것 (임시 폴더의 실제 `pi-store`, 가짜 시계. 검증 주석: E22 N2 P20 V42)
- 알림이 간 사기 → 답 없음 → `unhandled_due`에 `ReduceWithinLimit` 하나 → `Reduced { 3 }` 기록 → `unhandled_due` 빔, `due_handling`이 그 값. 두 번째 기록은 `AlreadyHandled`.
- 할 일과 맞지 않는 기록(`DoNotBuy`에 `Reduced`, 팔기에 `NotBought`), 수량 0 · 승인 수량보다 큰 수량은 `NotDue`.
- 기다림 · 승인됨 · 거절된 요청, 실전 승격 · 규칙 개선 · 다시 시작 요청에 기록하면 `NotDue`.
- 기록 전에 다시 켠 `Approvals`(같은 저장소)도 `unhandled_due`에 같은 것을 준다. 기록 뒤에 다시 켜면 비어 있고 `due_handling`이 남아 있다.
- 신호 유효 시간이 지난 뒤의 `unhandled_due`는 `Dropped`.
- `expire_due`의 반환은 지금과 같다(한 번 닫은 요청은 다시 돌려주지 않음).

### (e) 잠긴 옛 테스트와 부딪히는 곳
없음. `expire_due` · `on_restart` · `DueOutcome`을 바꾸지 않는다. 확인한 곳: `tests/suite/no_answer.rs:29-136`(할 일 갈래), `tests/suite/restart.rs`(다시 켠 뒤 만료).

### 물을 것
- 꺼졌다 켜진 사이 신호 유효 시간이 지난 답 없음 할 일을 `Dropped`로 둘지(추천), 그래도 팔기(`TeamDecides`)는 팀에게 넘길지.

## 하지 않는 것

알림 보내기(notify), 주문 내기(exec), 화면, 수량 다시 계산(rules).

## 테스트 주의

저장은 임시 폴더의 실제 `pi-store`로 한다. 동시에 두 번 `consume`해도 한 번만 통과하는지 본다. 시계는 가짜를 쓴다.

## 필수 번호

C7 C12 M5 E7 E8 E22 N2 V42 V43 V44 V45 P18 P19 P20

## 3차 작업: 개선안 승인을 journal 판정에 맡기기 (server W5 2차 보강에서 나옴)

조정자 결정(2026-11-16): 개선안 승인은 승인 소비를 이 모듈에 두되, 반영 판정(지금 규칙과 비교한 위험 재판정 → 냉각)은 journal의 `Journal::approve`가 하게 한다. 그러려면 소비한 승인(`ApprovalUse`)을 정해진 범위 안에서만 내준다.

- `Approvals::with_proposal_approval(number, proposal_id, f)` 같은 공개 메서드: 승인 번호의 기다림 · 지문(개선안 번호와 판) 확인 → 소비 → `f(ApprovalUse, at)` 호출 → `f`의 결과를 돌려준다. `ApprovalUse`는 이 호출 안에서만 쓰이고, `f`가 실패하면 소비를 되돌리지 않고(이미 쓴 승인) 오류를 돌려준다 — 기존 `approve_proposal`의 실패 규칙과 같게.
- 기존 `approve_proposal`은 그대로 둔다.
- 시험: 지문이 다른 개선안 · 이미 쓴 승인 · 거절된 승인으로는 `f`가 불리지 않는다; `f` 안에서 받은 `ApprovalUse`로 `Journal::approve`를 부르면 위험한 경우 냉각중이 된다(테스트 dev-dependency로 pi-journal 사용 가능).

## 새 store API 쓰기 (store 12차에서 넘어옴)
- 실전 승격은 `promote_template_live_checked`를 부른다(전체 정지 · 템플릿 판을 같은 묶음에서 본다).
