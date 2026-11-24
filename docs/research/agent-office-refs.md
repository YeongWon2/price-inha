# 사무실 화면 참고 조사: pixel-agents · Star-Office-UI · PixelOffice · ai-town

조사 방식: 공개 저장소를 `git clone --depth 1`로 받아 읽기와 정적 분석만 했다(스크립트 · 빌드 · 설치 실행 없음). 아래 경로는 모두 `scratchpad/refs/src/<이름>/repo/` 기준이다.
- pixel-agents: `4dd14d6` (2026-11-09), MIT
- Star-Office-UI(`star-office/`): `f29c107` (2026-03-11), 코드 MIT · 그림 비상업
- PixelOffice(`pixeloffice/`): `4b16d35` (2026-08-01), Apache-2.0. Kotlin + libGDX 데스크톱 앱
- ai-town: `5c91ce6` (2026-11-19), MIT. Convex + PixiJS

표기: `파일:줄`은 위 폴더 안의 경로다. "추정"이라고 쓴 것은 코드에서 직접 확인하지 못한 것이다.
수치 단위는 코드 그대로 둔다(초 · ms · px · 타일).

---

## 0. 한눈에 보는 비교

| 항목 | pixel-agents | Star-Office-UI | PixelOffice | ai-town |
|---|---|---|---|---|
| 에이전트 상태 수 | 3(idle · walk · type) + 말풍선 2종(permission · waiting) | 6(idle · writing · researching · executing · syncing · error) | 앱 입력 9 → 개발자 상태 15 | 상태 없음. 행동(걷기 · 활동 · 대화) + 생각/말하기 표시 |
| 상태 바꾸는 것 | Claude Code 훅 이벤트(즉시) + 훅이 없을 때만 추론 타이머 | 외부가 `/set_state`로 밀어 넣음 + TTL 자동 idle | 훅 이벤트(UDP · HTTP) + 30분 만료 | 서버 엔진이 1초 단위로 결정 |
| 말풍선 글 | 없음(아이콘만). 글은 라벨 패널 | 상태별 한국어가 아닌 중국어 문구 중 무작위, 3초 | 없음(그림 아이콘만) | 이모지(💭 💬)만. 글은 오른쪽 패널 |
| 길 찾기 | 4방향 BFS 타일 | 없음(직선 이동 · 칸 고정) | 직선 네트워크(통로 중심선) 위 BFS | 격자 A* 비슷한 탐색(MinHeap) |
| 한가할 때 | 무작위 배회 · 의자 휴식 | 없음(상태 따라 자리 이동) | 순찰 · 책상 방문 잡담 · 마주치면 잡담 | 활동(독서 등) · 대화 초대 · 배회 |
| 범례 | 없음 | 없음 | 없음 | 없음 |

네 곳 모두 말풍선 아이콘 범례가 없다. 우리가 범례를 두면 원본보다 나은 점이다.

---

## 1. pixel-agents (MIT)

### A. 상태 모델
- 캐릭터 상태는 3개: `idle` `walk` `type` (`webview-ui/src/office/types.ts:29`). 일하는지는 `isActive` 불리언이 따로 있다(`types.ts:~190`). 말풍선은 `bubbleType: 'permission' | 'waiting' | null`(`types.ts:~200`).
- 상태를 바꾸는 것:
  - 서버가 Claude Code 훅(`PreToolUse` `PostToolUse` `Stop` `UserPromptSubmit` `SubagentStart/Stop` `PermissionRequest` `Notification` `SessionStart/End`)을 받아 `agentStatus` `agentToolPermission` 메시지를 보낸다 (`server/src/providers/hook/claude/claude.ts:161-280`).
  - 훅이 안 오는 환경의 대체 추론 타이머: 권한 말풍선 7000ms(`server/src/constants.ts:15`), 글만 쓴 턴이 끝났다고 보는 침묵 5000ms(`:17`), 도구 완료 표시 지연 300ms(`:11`). 훅이 한 번이라도 도착하면(`hookDelivered`) 이 타이머들은 꺼진다(`server/src/hookEventHandler.ts:30`).
  - 폴링: 로그 파일 1000ms(`constants.ts:2`), 감시 500ms(`:3`), 외부 세션 탐색 3000ms(`:22`), 2분 안에 수정된 세션만 채택(`:24`), 사용자가 X로 닫으면 3분 쿨다운(`:30`).
- 상태 → 화면 매핑

| 상태 | 애니메이션 · 자리 | 근거 |
|---|---|---|
| 일하는 중(`isActive` + 자리 도착) | `type` 상태. 앉아서 타이핑 2프레임, 프레임당 0.3초. 도구가 읽기류(`Read` `Grep` `Glob` `WebFetch` `WebSearch`)이면 "읽기" 애니(프레임당 0.6초) | `constants.ts:13`, `:697`; `characters.ts:~97` |
| 일하러 가는 중 | `walk` 4프레임, 프레임당 0.15초, 자기 자리까지 BFS | `constants.ts:12`; `characters.ts:218` |
| 턴이 끝남 | 즉시 일어나 `idle`. 배회 시작 | `officeState.ts:909-925`(`seatTimer = -1` 표식) |
| 한가함 | `idle` 후 배회(아래 C) | `characters.ts:130` |
| 도우미(하위 에이전트) | 의자 없이 상위 에이전트 자리 반경 2타일에 머문다. 일하면 앉은 자세, 도구 호출 사이 3초 넘게 쉬면 일어선다 | `constants.ts:21,23`; `characters.ts:347` |
| 생겨남/사라짐 | 0.3초 "매트릭스 비" 효과 | `constants.ts:26` |

### B. 말풍선 · 표시
- 말풍선은 2종뿐이다(`types.ts` `bubbleType`).
  - `permission`(권한 필요): 노란 점 세 개. 사라지지 않고 도구가 끝나거나 권한이 정리될 때 지운다(`officeState.ts:987-1001`, `useExtensionMessages.ts:646`). 클릭하면 즉시 닫힘.
  - `waiting`(턴 끝남): 초록 체크. 수명 2.0초(`constants.ts:228`), 끝 0.5초 동안 서서히 사라짐(`:72`), 클릭하면 0.3초에 사라짐(`:229`).
  - 구분: 완료(`Stop`)면 체크만, 사용자 입력 대기(`idle_prompt`)면 체크 + 라벨 "Waiting for input"(`ToolOverlay.tsx:35,62`).
- 글 길이: 말풍선에는 글이 없다. 글은 머리 위 라벨 패널에 들어가며, 한 줄로 자르고 말줄임(`text-ellipsis`, `ToolOverlay.tsx:320,331`). 도구 문구는 서버에서 미리 자른다: 명령은 30자(`core/src/constants.ts:21`), 하위 작업 설명은 40자(`:22`). 예: "Reading foo.ts" "Running: npm test…" "Searching code" (`claude.ts:30-60`).
- 겹침 처리: 라벨은 기본이 "호버 또는 선택된 에이전트만" 보인다(`ToolOverlay.tsx:198`, 항상 보기 옵션 `App.tsx:162` 기본 꺼짐). 말풍선이 뜨면 라벨이 말풍선 위로 쌓인다(`labelBottom = min(screenY, bubbleTop)`, `ToolOverlay.tsx:209`). 하위 에이전트에 호버하면 상위 라벨을 숨긴다(`:140`).
- 알림음(Web Audio 사인파, 설정에서 끌 수 있음 `notificationSound.ts:17,34`):
  - 완료: E5(659.25Hz) → E6(1318.51Hz), 시작 0 / 0.1초, 음당 0.18초, 음량 0.14 (`constants.ts:204-210`).
  - 권한: A5(880Hz) → E5(659.25Hz), 시작 0 / 0.12초, 음당 0.15초, 음량 0.12 (`:212-218`).
  - 호출 위치: 완료는 `agentStatus='waiting'`일 때(`useExtensionMessages.ts:633-634`), 권한은 `agentToolPermission`일 때(`:646-647`).
- 머리 위 아이콘: 위 2종 + 펫 하트. 이름표 옆 상태 점: idle 초록 · working 파랑 · attention 노랑 · disconnected 빨강(`components/statusDot.ts`). 범례 없음.

### C. 이동
- 길 찾기: 4방향 BFS, 대각선 없음(`office/layout/tileMap.ts:37`). 큐는 배열 `shift`로 구현. 타일 16px.
- 속도: 48px/초(= 3타일/초, `constants.ts:11`). 펫은 32px/초(`:333`).
- 한가할 때(`characters.ts:130-215`):
  - 배회 간격 2.0~20.0초 무작위(`constants.ts:14-15`).
  - 한 번 일어나면 걷기 3~6번(`:16-17`) 한 뒤 자기 자리로 돌아와 앉는다.
  - 자리에서 쉬는 시간 120~240초(`:18-19`) 뒤 다시 일어나 배회.
  - 배회 목표는 걸을 수 있는 타일 중 무작위.
- 서로 만났을 때: 에이전트끼리는 특별한 동작이 없다(막힌 타일 집합으로 피함). 펫만 사람 따라가기: 확률 0.3, 반경 3타일, 5~15초(`:349-355`).

### D. 화면 구성
- 레이아웃: 사무실 캔버스 위에 DOM 오버레이(라벨 · 말풍선). 왼쪽 위에 버튼 열 + 사용량 막대("하트" 모양) `usage/UsageBars.tsx`, 에이전트 카드 목록(`components/AgentCardList.tsx`, 카드마다 상태 점 · 맥락 막대), 하단 툴바(`BottomToolbar.tsx`), 설정 모달, 오른쪽 대화 기록 사이드바(`conversations/ConversationSidebar.tsx`, 폭 `min(340px, 85vw)`, 열림 340ms · 닫힘 200ms: `constants.ts:751,737,742`).
- 이름표/라벨: 캐릭터 머리 위(`TOOL_OVERLAY_VERTICAL_OFFSET = 32`, `constants.ts:237`). 팀 리드는 금색 "LEAD"(`:306`).
- 맥락 잔량 막대: 라벨 옆 40×4px, 사용률 0.33 미만 초록 · 0.66 미만 노랑 · 그 이상 빨강(`constants.ts:300-312`). 카드 바닥에 3px 막대.
- 사용량 막대(계정 한도): 남은 비율 0.5 이하 경고색, 0.2 이하 위험색(`:802-806`). 30초마다 시계 갱신(`:809`).
- 구역 이름: 구역("Area") 중심에 흰 글자 14px(최소 12px) + 검은 그림자 0.6(`:164-174`).
- 모바일: `(max-width: 768px), (pointer: coarse) and (max-width: 1024px)`(`constants.ts:490`). 모바일 전용 카드 바 · 키 바 · 화면 전환 300ms(`components/MobileAgentBar.tsx`, `MobileKeyBar.tsx`, `constants.ts:492`).

### E. 실패 · 성공 연출, 절약 장치
- 실패 전용 연출은 없다. 연결이 끊기면 상태 점이 빨강(disconnected). 성공 = 체크 + 완료음.
- 프레임 시간 상한 0.1초로 끊어 탭이 멈췄다 돌아와도 순간이동하지 않는다(`gameLoop.ts:25`, `constants.ts:227`). 렌더 루프는 숨김 탭에서 멈추지 않는다(`requestAnimationFrame`에 맡김, 직접 정지 코드 없음).
- 유휴 시뮬레이션 정지는 없다. 음악 엔진만 숨김 시 미리 예약 구간을 0.2 → 1.5초로 늘린다(`constants.ts:826-828`).
- 오프라인 복구: 웹소켓 재연결 지연 250/500/1000/2000/4000ms(`:372`).

---

## 2. Star-Office-UI (코드 MIT, 그림 비상업)

### A. 상태 모델
- 허용 상태 6개: `idle` `writing` `researching` `executing` `syncing` `error` (`backend/app.py:72`). 동의어는 정규화(`working/busy/write`→writing, `run/running/exec`→executing, `sync`→syncing, `research/search`→researching, 모르면 idle: `backend/app.py:563-581`).
- 상태를 바꾸는 것: 외부(에이전트 스크립트)가 `/set_state`에 상태를 밀어 넣는다. 밀어 넣기 간격 15초(`office-agent-push.py:24`). 화면은 `/status`를 2000ms마다(`frontend/game.js:196,704`), `/agents`를 2500ms마다(`:204`) 가져온다.
- 자동 복귀: 일하는 상태(writing · researching · executing)에서 `ttl_seconds`(기본 300초, `backend/app.py:176`) 동안 갱신이 없으면 idle로 되돌리고 저장한다(`:188`). 주석에는 25초라고 쓰여 있지만 코드 기본값은 300초다(불일치).
- 손님 에이전트: 5분(300초) 동안 푸시가 없으면 `offline`(`:878`). 접속 키당 동시 접속 상한 기본 3(`:1024`).
- 상태 → 구역 매핑(`backend/app.py:74-81`, `frontend/layout.js` areas)

| 상태 | 구역 | 좌표(1280×720) |
|---|---|---|
| idle | breakroom(휴게실, 소파) | (640, 360) |
| writing / executing / syncing | writing(책상) | (320, 360) |
| researching | researching(프런트 문구상 writing과 같은 좌표) | (320, 360) |
| error | error(서버룸 버그) | (1066, 180) |
| (문) | door | (640, 550) |

  syncing은 동기화 애니(`syncAnim` 위치 1157,592), error는 버그 스프라이트가 좌우 왕복(1007↔1111, 속도 0.6/프레임)한다(`layout.js`).

### B. 말풍선 · 표시
- 언제: 상태가 idle이 아닐 때 8000ms마다 상태별 문구 목록에서 무작위 1개(`game.js:198,675`). idle일 때는 안 띄운다(`showBubble`: `if (currentState === 'idle') return;`, `:871`).
- 수명 3000ms(`:896`). 고양이 말풍선은 18000ms마다, 수명 4000ms(`:199`, `:911`).
- 크기: 가로 = 글자 수 × 10 + 20px, 높이 28px, 머리 위 70px, 글꼴 12px(`:890-893`). 새 말풍선이 뜨면 이전 것을 지운다(`:872`) → 겹침 없음.
- 상태 문구(`BUBBLE_TEXTS`)는 "상태 설명 + 농담" 성격이라 정보 가치는 낮다. 정보는 아래 "상태 줄(typewriter)"에 있다: 글자당 50ms 타자기 효과로 `[상태 이름] 상세` 한 줄(`:201,727`).
- 알림음 없음. 머리 위 아이콘 없음(상태 점만: 승인 초록 · 대기 노랑 · 거절 빨강 · 오프라인 회색, `game.js:~960`). 범례 없음.

### C. 이동
- 길 찾기 없음. 주 캐릭터는 목표로 직선 이동(속도 1.4px/프레임, 가볍게 위아래 흔들림 `sin(t/200)*0.8`, `game.js:801-806`). 경유점 배열 `waypoints`는 코드에 소비 로직만 있고 채우는 곳을 찾지 못했다(`game.js:192,810`). 상태 바뀌면 자리 스프라이트로 교체되는 방식이 주.
- 손님 에이전트는 구역별 8칸 고정 위치 목록에 인덱스로 배치(`AREA_POSITIONS`, `game.js:224-258`, `슬롯 % 8`). 이동 · 순찰 · 잡담 없음. 고양이는 제자리.

### D. 화면 구성
- 머리 위에 이름표: 글자 14px, 검은 테두리 3px, 흰 배경 0.95, 캐릭터 중심에서 위로 36px(`game.js:985`). 글자색 = 승인 초록 · 대기 주황 · 거절 빨강 · 오프라인 회색. 반투명: 대기 0.7 · 거절 0.4 · 오프라인 0.5. 상태 점은 오른쪽 위(20, -20), 반지름 5.
- 하단: 상태 줄(`#status-text`), `#bottom-panels`에 "어제 메모"(`#memo-panel`, `yesterday-memo`) · 상태 버튼(`#control-bar`) · 손님 목록(`#guest-agent-panel`)(`frontend/index.html:1145-1176`). 어제 메모는 마크다운에서 핵심 3줄만 뽑고 개인정보(이메일 · IP · 경로 · 전화)를 가려서 보여 준다(`backend/memo_utils.py:20-60`).
- 구역 이름 표시 없음(장식이 곧 구역).
- 로딩 화면: 에셋 15개 진행 막대(`layout.js totalAssets`, `game.js` `updateLoadingProgress`).
- 모바일: `@media (max-width: 900px), (pointer: coarse)`에서 사무실을 화면 높이의 2/3(66.666vh)로 고정하고 그 아래 패널을 세로로 쌓는다(`index.html:959-990`).

### E. 실패 · 성공 연출, 절약 장치
- 실패(error): 서버룸 쪽 버그 스프라이트 왕복 + 캐릭터를 error 구역(왼쪽 위 서버 쪽)으로. 성공 전용 연출 없음.
- 절약: TTL 자동 idle, 5분 오프라인 처리, 정적 파일 캐시 헤더(`app.py:135`). 시뮬레이션 정지는 없다.

---

## 3. PixelOffice (Apache-2.0) — 우리와 가장 가까운 모델

### A. 상태 모델
- 외부 입력은 상태 9개: `idle thinking planning researching coding running waiting success failure`와 이벤트 종류 `upsert / touch / end`(`core/.../integration/AgentEvents.kt`). 신분은 `provider:sessionId:agentId`(`AgentSessionStore.kt:key`). 오래된 이벤트는 버림(`occurredAt < lastActivityAt`, `AgentSessionStore.kt:14`).
- 훅 → 상태(`hooks/pixel_office_hook.py:77-176`)
  - `SessionStart`→idle, `SessionEnd`→end, `UserPromptSubmit`→thinking(계획 모드면 planning)
  - `PreToolUse`: 사용자 질문 도구→waiting(`:20`), 계획 도구→planning, 편집 도구→coding, 읽기 · 검색→researching, Bash→running(명령에 테스트/빌드/커밋/설치 표지 분류)
  - `PermissionRequest`→waiting(permission)
  - `PostToolUse`→thinking(도구가 끝나면 모델이 다시 생각하니까). 테스트 · 빌드면 구조화된 성공 여부로 success/failure
  - `PostToolUseFailure`→failure, `Stop`→idle, `SubagentStart`→thinking, `SubagentStop`→end
  - 프롬프트 · 명령 · 파일 내용은 보내지 않는다(상태 패킷만).
- 받는 쪽 상태기계(`states/DeveloperStates.kt`, 이벤트 → 상태): 15개 상태. 매핑(`AgentStateMapper.kt:5`): thinking→`thinking_started`, planning→`planning_started`, researching→`researching_started`, coding→`code_writing_started`, running→`command_started`, waiting→`waiting_started`, success→`command_succeeded`, failure→`tests_failed`.
- 만료: 마지막 활동 후 30분이면 세션 삭제(`Config.kt:223`, `staleSessionMinutes = 30`), 5초마다 점검(`PixelOfficeGame.kt:46`).
- 상태 → 연출 매핑

| 상태 | 애니메이션 | 머리 위 아이콘 | 자리 | 근거 |
|---|---|---|---|---|
| idle | idle | 없음 | 자기 책상 | `DeveloperStates.kt` IdleState |
| thinking · researching | thinking/idle | 생각 점 | 책상 | 같은 파일 |
| planning | 걷기 → 화이트보드 앞에서 thinking | 생각 점 | 화이트보드(3곳, 한 번에 한 명이 점유) | `:128` AtWhiteboardState |
| coding | typing(4~8초) ↔ reading(1.5~3초) 번갈아 | 코딩 아이콘 | 책상 | `:254-277` |
| running | idle | 없음 | 책상 | RunningCommandState |
| building | idle | 톱니 | 책상 | BuildingState |
| success | idle | 엄지(1.5초) 후 thinking | 책상 | `:391` CelebratingState |
| failure(tests_failed) | despair(절망) + 유령 소환, 2.0초 후 화이트보드로 | (유령 효과) | 화이트보드로 이동 | `:489` |
| ci_failed | idle | 유령 | 책상 | CiFailedState |
| waiting | idle | 짜증("annoyed") | 책상 | `:519` |
| being_interrupted | idle | 짜증, 3.0초 후 직전 상태로 복귀 | 책상 | `:548` |

### B. 말풍선 · 표시
- 말풍선 종류(그림): thinking · blah(잡담) · question · annoyed · coding · gear · ghost · thumbsup/success (`rendering/Renderer.kt:1195-1211`). 글자 없음. 4프레임 애니(프레임 0.3초), 위아래로 2px, 속도 3의 사인 흔들림(`entities/ThoughtBubble.kt`).
- 수명: 상태에 묶인다. 상태에 들어갈 때 뜨고 나갈 때 지운다(`showBubbleOfType` / `showThoughtBubble(false)`: `Developer.kt:430`). 성공 1.5초 · 실패 2.0초 · 끼어들기 3.0초만 시간 제한.
- 위치: 의자 기준으로 좌우 ±16px, 위로 26px(`Developer.kt:106-119`). 겹침 처리 없음(캐릭터당 말풍선 1개이며 새 종류가 이전 것을 대체).
- 알림음: 없음(`grep sound` 결과 없음, 추정). 범례: 없음.
- 프로젝트 이름은 사무실 위쪽에 문자로만(`Renderer.kt:2386 drawScene projectLabel`). 캐릭터 이름표 없음 — 라벨 표시 모드는 디버그용(F4: OFF/가구/경로, `Renderer.kt:2026`).

### C. 이동
- 길 찾기: "선 네트워크". 통로 중심선 · 책상 접속점 · 화이트보드 접속점을 격자 대신 직교 선분 그래프로 만들고 BFS(`world/LinePathfinder.kt`, `LineNetwork.kt:406`). 시작/끝은 가장 가까운 접속점에 붙이고, 첫 점이 8px 안이면 건너뜀. 경로를 못 찾으면 직선.
- 속도: 40(화면 320×240 기준 픽셀/초, `Config.kt:163`). 펫 20(roam 60~120초, `assets/config.json pets`).
- 한가할 때(`world/Office.kt:35-54, 684-960`): 자기 책상에서 `idle`인 채로 20~45초(`Config.kt:167-170`) 지나면 순찰 시작 → 걷기 → 정차 → 최대 3곳(`idlePatrolMaxStops`) → 책상으로 돌아감.
  - 순찰 목표: 자기 책상이 아니고 아직 안 간 책상 중, 사람이 앉아 있고 대화 중이 아닌 책상을 우선(`choosePatrolDesk`, `:760`).
  - 도착했는데 사람이 있으면 "책상 방문 대화": 방문자 말풍선 blah/question, 앉아 있던 사람은 `being_interrupted`(짜증), 3.0초(`socialDurationSeconds`, `Config.kt:~175`).
  - 사람이 없으면 3.0초 멈춰 생각/질문 말풍선(`:778`).
- 서로 만났을 때: 순찰 중 두 사람 사이 거리 15px 미만이면 둘 다 멈추고 blah 말풍선 3.0초(`Office.kt:80,935-967`).
- 실제 일이 오면 순찰 · 대화를 즉시 취소하고 일부터 한다(`beforeDeveloperAgentEvent`, `Office.kt:662`).
- 프로젝트별 책상/가구 변주는 프로젝트 ID 해시로 고정(`stableVariantFor`, `:669`).

### D. 화면 구성
- 해상도 320×240 기준 30fps(`assets/config.json display`). 프로젝트는 3열 격자로 사무실을 복제(`gridColumns = 3`, `Config.kt:226`), 위쪽에 공용 하늘(구름 · 비행기 · UFO 장식 `SkyTraffic.kt`).
- 이름표 · 하단 상태줄 · 패널 · 구역 이름: 없음(그림만). 설정 오버레이만 있음(`ui/SettingsOverlay.kt`). 모바일 대응: Android 런처 모듈이 있으나 화면 대응은 확인 못 함(추정: 같은 렌더).

### E. 실패 · 성공 연출, 절약
- 성공: 엄지 아이콘 1.5초. 실패: 절망 애니 + 유령 2.0초 + 화이트보드로 가서 생각. 빌드: 톱니. 머지: 엄지.
- 절약: 시뮬레이션 정지 장치 없음. 만료(30분) 세션 삭제만 있다.
- LAN 수신은 인증 없음(문서 경고, `README.md`). 우리는 해당 없음.

---

## 4. ai-town (MIT)

### A. 상태 모델
- 에이전트에 상태 이름은 없다. 한 에이전트가 매 틱에 하는 일(`convex/aiTown/agent.ts:60-140`):
  - 대화 중이 아니고 · 활동 중이 아니고 · (이동 중이 아니거나 최근 초대를 안 했으면) → `agentDoSomething` 연산 시작
  - 대화 상태: `invited → walkingOver → participating → left`(`conversationMembership.ts`)
  - 활동: 독서 📖 · 몽상 🤔 · 텃밭 🥕, 각각 60초(`convex/constants.ts:67-71`)
- 바꾸는 것: 서버 엔진 틱 16ms · 스텝 1000ms(`constants.ts:5-6`). 사람 입력은 입력 지연 1000ms(`:30`). 클라이언트는 서버 이력을 보간해 부드럽게 그린다(`useHistoricalTime`, `Game.tsx:35`).
- 상태 → 표시 매핑: 걷기(`speed > 0`)면 걷기 애니 재생, 멈추면 정지(`Character.tsx isMoving`). 생각 💭 = 에이전트에 진행 중 연산이 있고 말하는 중이 아닐 때(`Player.tsx:60-64`), 말하기 💬 = 대화에서 입력 락을 쥔 사람(`Player.tsx:57-59`), 활동 이모지 = 활동 중일 때(`Character.tsx` 머리 위 -24px).

### B. 말풍선 · 표시
- 머리 위에 이모지만: 💭(왼쪽 -20,-10) · 💬(오른쪽 18,-10) · 활동 이모지(위 -24), 크기 0.8배(`Character.tsx:88-103`). 글은 오른쪽 패널의 메시지 목록에 쌓인다.
- 말풍선 수명: 상태에 묶임(입력 락 쥐는 동안). 입력 락 타임아웃 15초(`TYPING_TIMEOUT`, `constants.ts:11`). 패널에는 `typing...` 표시(`Messages.tsx:142-151`).
- 글 길이/횟수: 대화 하나에 최대 8메시지(`MAX_CONVERSATION_MESSAGES`), 최대 10분(`MAX_CONVERSATION_DURATION`), 메시지 사이 읽는 시간 2000ms(`MESSAGE_COOLDOWN`), 상대가 말 없으면 60초 뒤 먼저 말함(`AWKWARD_CONVERSATION_TIMEOUT`) (`constants.ts:20-27`; `agent.ts:112-160`).
- 알림음 없음(배경 음악만). 범례 없음.

### C. 이동
- 길 찾기: 격자에서 맨해튼 거리 휴리스틱의 최단 경로 탐색(MinHeap, `movement.ts:57-160`). 한 스텝에 길찾기는 최대 16건(`MAX_PATHFINDS_PER_STEP`, `constants.ts`). 시간 초과 60초, 재시도 대기 1000ms.
- 속도: 0.75 타일/초(`data/characters.ts:121`).
- 한가할 때: `agentDoSomething`이 활동을 고르거나 배회 목적지를 정하고, 대화 후보를 찾아 초대한다(`agentOperations.ts:100-175`). 대화 직후/초대 직후 15초(`CONVERSATION_COOLDOWN`)는 새 대화를 안 건다. 활동 후 10초 쿨다운(`ACTIVITY_COOLDOWN`).
- 서로 만났을 때: 에이전트끼리는 초대 수락 확률 0.8(`INVITE_ACCEPT_PROBABILITY`), 사람은 무조건 수락(`agent.ts:~95`). 수락하면 서로 가까워지는 쪽으로 걷는다. 거리 1.3타일 이내면 대화 시작(`CONVERSATION_DISTANCE`), 멀면 두 사람 중간점으로 걷고 4타일 안이면 상대에게 직접(`MIDPOINT_THRESHOLD = 4`). 초대 후 60초 안 되면 포기(`INVITE_TIMEOUT`). 충돌 임계 0.75(`COLLISION_THRESHOLD`).

### D. 화면 구성
- 화면: 지도(PixiJS) + 오른쪽 패널(`lg:w-96`, 384px)에 선택한 캐릭터 상세 · 대화 메시지 · 입력창(`Game.tsx:45,70`). 모바일(lg 미만)은 지도 240px 높이 + 아래에 패널(`grid-rows-[240px_1fr]`, `Game.tsx:45`).
- 이름표 · 하단 상태줄 · 구역 이름 없음. 선택된 사람 캐릭터 발 아래에 노란 타원 표시(`ViewerIndicator`, `Character.tsx:~110`, 알파 0.5).
- 로그/메모리: 대화가 끝나면 기억으로 저장하는 연산이 따로 돈다(`agentRememberConversation`).

### E. 실패 · 성공, 절약
- 실패: 에이전트 연산 타임아웃 120초(`ACTION_TIMEOUT`, `constants.ts:1`). 화면 연출 없음.
- 절약(핵심): 화면을 보는 클라이언트가 60초마다 하트비트를 보낸다(`WORLD_HEARTBEAT_INTERVAL`, `world.ts:41`). 5분(`IDLE_WORLD_TIMEOUT`) 보는 사람이 없으면 세계를 `inactive`로 멈추고(`crons.ts:11-12`), 다시 열면 재시작(`world.ts:47`). 개발자 정지(`stoppedByDeveloper`)는 자동 재시작 안 함. 오래 멈춘 만큼 한꺼번에 시뮬레이션하지 않도록 `MAX_STEP`(10분)으로 자름(`constants.ts:6`). 오래된 데이터 청소 2주(`VACUUM_MAX_AGE`).

---

## F. 우리에게 넣기를 권하는 것 (상위 8)

전제: 우리는 3D 사무실이고 AI 6명, 서버가 상태를 정확히 안다(원본처럼 훅을 추정할 필요가 없다). 따라서 "추론 타이머"는 가져오지 않고, "표시 규칙 · 수치 · 안전장치"를 가져온다. 어떤 저장소의 코드도 옮기지 않고 동작만 같게 만든다. 그림 · 효과음 · 글꼴 파일은 하나도 쓰지 않는다(3D라서 어차피 못 쓴다).

| # | 권장 | 근거 저장소 · 원본 수치 | 우리 쪽 한 줄 | 자산/라이선스 |
|---|---|---|---|---|
| 1 | **말풍선 2종 분리: "내 응답 필요(승인 · 입력)"는 지울 때까지 유지, "끝났다"는 2.0초 + 0.5초 페이드, 클릭하면 0.3초에 닫힘** | pixel-agents: 권한 말풍선 무기한(`officeState.ts:987-1001`), 완료 2.0s · 페이드 0.5s · 클릭 0.3s(`constants.ts:72,228-229`) | 머리 위 말풍선 컴포넌트에 `kind: needs_user \| done`과 수명 규칙을 그대로 넣는다. 오른쪽 채팅 패널에 같은 알림이 남게 한다 | MIT 코드. 동작만 따라 한다. 그림은 쓰지 않는다 |
| 2 | **말풍선 아이콘 어휘 + 범례**: 생각 점 · 코딩/분석 · 톱니(실행) · 엄지(성공, 1.5s) · 유령/망연(실패, 2.0s) · 짜증(입력 대기) · 물음표 · 잡담 + 작은 범례(네 곳 모두 범례 없음) | PixelOffice: `Renderer.kt:1195-1211` 아이콘 8종, 성공 1.5s · 실패 2.0s · 끼어들기 3.0s(`DeveloperStates.kt:391,489,548`) | 투자 AI의 상태(분석 중 · 주문 검토 · 승인 대기 · 체결 성공 · 실패)를 아이콘 8개 안에서 매핑하고, 사이드 패널 아래에 범례를 한 줄로 둔다 | Apache-2.0(코드). 스프라이트 출처는 저장소에 명시가 없어 확인 못 함 → 그림 재사용 금지 |
| 3 | **한가할 때 순찰 · 잡담 + "진짜 일이 오면 즉시 취소"**: idle 20~45초 → 최대 3곳 방문, 3.0초 대화, 마주치면(15px ≈ 1타일) 3.0초 잡담 | PixelOffice: `Config.kt:167-175`, `Office.kt:684-967`, 취소 `Office.kt:662`. pixel-agents 대안: 배회 2~20s · 3~6회 · 자리 휴식 120~240s(`constants.ts:14-19`) | 6명이 idle일 때 자리 → 라운지/회의실 → 다른 AI 자리를 방문. 서버의 실제 상태 이벤트가 오면 모든 사회 행동을 취소(순서 보장). 6명이라 동시 대화는 쌍 하나 정도면 충분 | 동작 복제만. 라이선스 문제 없음(코드 안 씀) |
| 4 | **상태 → 구역 표 + 구역별 고정 슬롯**: idle→라운지, 일함→자리, 실패→전용 구역, 구역마다 슬롯 N개를 두고 `슬롯 % N`로 겹치지 않게 배치 | Star-Office: `backend/app.py:74-81`, `frontend/layout.js`, `game.js:224-258`(구역당 8슬롯) | 우리 3구역(자리 · 회의실 · 라운지)에 상태 표를 확정하고 슬롯 6~8개를 정의. 회의실은 "여럿이 협의 중"에 사용 | MIT 코드. 그림은 비상업 → 사용 금지 |
| 5 | **이름표 + 잔량 막대 + 상태 점 색 규칙**: 라벨은 호버/선택 때만 상세, 기본은 이름 + 점. 점 색 idle 초록 · working 파랑 · attention 노랑 · disconnected 빨강. 막대 40×4px, 사용률 0.33/0.66에서 초록 → 노랑 → 빨강. 오프라인은 알파 0.5 | pixel-agents: `statusDot.ts`, `constants.ts:300-312`; Star-Office: 알파 0.5(오프라인) 0.7(대기) `game.js:~950` | 발 아래 이름표 옆에 점 하나, 이름 아래 가는 막대 하나(토큰/예산 잔량). 활동 텍스트는 30자/40자에서 말줄임(`core/src/constants.ts:21-22`) | MIT 코드. 라벨 위치(원본은 머리 위)는 우리 설계(발 아래)와 다르니 수치만 참고 |
| 6 | **채팅 패널 규칙**: AI가 응답 생성 중이면 머리 위 "생각" 아이콘, 입력(스트리밍) 중이면 "말하기" 아이콘 + 패널에 `typing...`, 메시지 사이 최소 2.0초(읽는 시간), 상대가 침묵하면 60초 뒤 먼저 말함 | ai-town: `Player.tsx:57-64`, `Character.tsx:88-100`, `Messages.tsx:142-151`, `constants.ts:20-27` | 오른쪽 패널 폭은 384px을 시작값으로(ai-town `lg:w-96`, pixel-agents 사이드바 340px). 모바일은 3D 높이 240px 위 + 아래 패널(ai-town) 또는 66.666vh(Star-Office)로 | MIT 코드. 이모지 💭💬는 글꼴 자산이라 그대로 쓰지 말고 우리 아이콘으로 |
| 7 | **실행 중 상태 표시의 정확한 의미 분리 + 오래된 working 자동 해제**: 도구 호출 사이는 "일어나지 않는다"(3초 유예), TTL 안에 갱신 없으면 idle로 복귀, 만료 세션은 5초 주기로 정리 | pixel-agents: 도우미 3초 유예(`constants.ts:23`); Star-Office TTL 300초(`app.py:176-188`); PixelOffice 만료 30분 · 점검 5초(`Config.kt:223`, `PixelOfficeGame.kt:46`), 오래된 이벤트 무시(`AgentSessionStore.kt:14`) | 서버에서 상태 이벤트에 단조 증가 시각을 붙여 늦게 온 이벤트는 버린다. "작업 중" 표시는 TTL(예: 5분, 우리가 정할 값) 후 자동 idle. 깜빡임 방지로 앉기/서기 유예를 둔다 | 코드 복사 없음. 라이선스 문제 없음 |
| 8 | **절약 장치**: 화면을 안 볼 때(탭 숨김 · 접속자 없음) 시뮬레이션 · 렌더 정지, 프레임 dt 상한 0.1초, 하트비트 60초 + 5분 무시청 정지, 재연결 지연 250→4000ms | ai-town: `Game.tsx:31`, `WORLD_HEARTBEAT_INTERVAL 60s`, `IDLE_WORLD_TIMEOUT 5min`(`constants.ts:3-4`, `crons.ts:11`); pixel-agents: `MAX_DELTA_TIME_SEC 0.1`(`constants.ts:227`), 재연결(`:372`) | 3D 렌더 루프는 `visibilitychange`로 정지(네 곳 모두 렌더는 안 멈춤 → 우리가 더 낫게 만들 부분). 서버는 접속자 없으면 순찰 시뮬 정지, 실제 투자 AI 동작은 계속 | MIT 코드. 문제 없음 |

추가 후보(우선순위 낮음): 생겨남/사라짐 0.3초 효과(pixel-agents `MATRIX_EFFECT_DURATION_SEC`), 알림음 2종(아래).
- 알림음 수치(옵션, 설정에서 끌 수 있게): 완료 E5 → E6 0.18초 음량 0.14, 권한 A5 → E5 0.15초 음량 0.12(`constants.ts:204-218`). 사인파는 코드로 만든 소리이므로 저작권 걱정은 없다. 브라우저는 사용자 제스처 전에는 소리가 안 나므로 `AudioContext.resume()` 해제 장치가 필요하다(`notificationSound.ts:75,116`).

### 라이선스 요약
| 저장소 | 코드 | 그림/소리 | 결론 |
|---|---|---|---|
| pixel-agents | MIT(`LICENSE`) | 캐릭터 그림은 JIK-A-4 "Metro City" 무료 팩에 기반(README), 가구는 Donarg "Office Interior Tileset" 사용을 권장(`docs/external-assets.md:181`). 각 팩의 라이선스는 이 조사에서 확인하지 못함 | 그림 재사용 금지. 동작만 |
| Star-Office-UI | MIT | 미술은 **상업 이용 금지**(`README.md:248-253`), 손님 캐릭터는 LimeZu 무료 팩(재배포 시 출처 표기) | 그림 사용 금지. 코드도 옮기지 않음 |
| PixelOffice | Apache-2.0 | 스프라이트(`assets/sprites/`)의 출처가 README에 없음 | 그림 사용 금지(출처 불명). 코드는 Apache라 허용되지만 어차피 옮기지 않음 |
| ai-town | MIT | 타일셋은 OpenGameArt(George Bailey · hilau), ansimuz, Mounir Tohami GUI 등 제각각(`README.md:43-56`). 음악은 MusicGen 생성 | 그림 · 음악 사용 금지 |

---

## 부록: rea(6.7.0)가 한 일 / 못 한 일

| 단계 | 명령 | 결과 | 비고 |
|---|---|---|---|
| 점검 | `rea doctor` | `healthy: false`. 원인: Hopper · IDA 미설치(네이티브 바이너리용). Node 26.12.1 · Ghidra 12.1.4 등은 정상 | 이번 작업은 소스 트리 분석이라 영향 없음 |
| 소스 가져오기 | `rea import-reference-source <repo> --detail full` ×4 | 4개 모두 성공. `HistoricalSourceGraph`(authority `historical-reference`, `inventory_state: partial`) JSON 36KB~568KB | 파일 목록과 분류 · 관계만 줌. 동작 수치는 안 줌 |
| JS 정적 분석 | `rea analyze-javascript-application <repo> --integrity-policy record-and-continue` ×3(pixel-agents · Star-Office · ai-town) | 성공. 결과 JSON 215MB · 68MB · 157MB. pixel-agents: 파싱한 JS/TS 495개(파싱 실패 6), ai-town 126개(실패 1), Star-Office 7개(번들 모듈 1625개 포함 = 동봉된 `phaser-3.80.1.min.js`) | PixelOffice(Kotlin)는 JS 분석 대상이 아니라 건너뜀 |
| 쓸모 있던 것 | 시맨틱 그래프에서 타이머 · 리스너 · fetch · 환경변수 위치 추출(파이썬 `-I`) | Star-Office: `/status` · `/set_state` fetch 위치(`game.js:704,260`), 타이머 값 3000 · 4000 · 5000(전자 셸 사본). pixel-agents: 환경변수 `PIXEL_AGENTS_DEBUG`(`extension.ts:16`), 이벤트 `agentAdded/agentRemoved/broadcast`(`PixelAgentsViewProvider.ts:161-167`). ai-town: `fetch` 위치(`llm.ts` · `music.ts`) | 확인용 보조 근거로만 썼다 |
| 못 한 것 | 상태 기계 · 문자열 값 · 수치 · 상수 이름 | 시맨틱 노드 상한(100000)에 걸려 pixel-agents와 ai-town은 일부만 보존. pixel-agents는 `webview-ui/` 노드가 0개(상한 초과로 잘림)라 렌더 · 캐릭터 로직은 못 얻음. 타이머 지연값은 상수 참조라 대부분 `null` | grep과 파일 읽기로 모두 채웠다(위 파일:줄) |
| 추가 한계 | 그래프 `unknowns` 11k~36k, 일부 모듈 해석 불가 | | 정적 분석이므로 런타임 동작(화면 모습)은 못 봄 |

## 확인하지 못한 것
- 각 저장소의 실제 화면 모습(실행하지 않음). 이름표 위치 · 말풍선 크기는 코드 수치로만 확인.
- pixel-agents 캐릭터 · 가구 그림 원본 팩(Metro City, Office Interior Tileset)의 라이선스 조건, PixelOffice 스프라이트 출처, ai-town 캐릭터 시트(`32x32folk.png`)의 출처와 조건.
- Star-Office의 경유점(`waypoints`)을 채우는 코드: 이 조사에서 찾지 못함. 주 캐릭터가 걷는 동작이 실제로 쓰이는지 불명.
- Star-Office `ttl_seconds`: docstring은 25초, 코드 기본은 300초. 실제 배포에서 호출 측이 값을 넣는지 확인하지 못함.
- PixelOffice에 알림음이 없다는 것은 `grep`으로만 확인(추정), 모바일(Android) 화면 대응은 확인 못 함.
- pixel-agents 렌더 쪽 말풍선 스프라이트 크기(px)와 `renderer.ts`의 그리기 세부는 읽지 않았다(라벨 · 말풍선 DOM 위치만 확인).
- 네 곳 모두 알림 범례가 없다는 결론은 파일 이름 · `legend` 검색 기준(`grep -ri legend`)이며 UI 문구 전수 확인은 아님.
