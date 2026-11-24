# 메타버스 느낌 레퍼런스 조사 (내 캐릭터 · 멘션 · 공지 · 말 걸기/부르기)

조사 방식: 정적 읽기만(스크립트 · 빌드 · 설치 실행 없음). 코드는 옮기지 않고 동작 · 수치 · 근거(파일:줄)만 적는다.
기준 커밋: deskrpg `5d80963`(`src/` 기준 경로), workadventure `b706ab8`(2026-11-24 shallow), SkyOffice `3f66b8b`(2025-12-19 shallow).

| 저장소 | 라이선스(확인한 것) | 우리에게 의미 |
|---|---|---|
| dandacompany/deskrpg | Sustainable Use License 1.0 (`LICENSE.md:1-8`) | 동작만 같게, 코드는 옮기지 않는다 |
| workadventure/workadventure | AGPL-3.0 + Commons Clause (`back/LICENSE.txt` 앞부분), Enterprise 판은 별도 상용 | 코드 복사 금지, 수치 · 동작 참고만 |
| kevinshen56714/SkyOffice | MIT (`LICENSE:1`) | 참고 자유 |

핵심 발견 5가지(먼저 읽기):
1. deskrpg는 키보드 이동이 **방향키뿐**이다(WASD 없음, 달리기 키 없음). WASD 문자열은 `office-renderer.ts:198`의 "칸반 도착 취소" 키 목록에만 있다. WA와 SkyOffice는 WASD 지원.
2. deskrpg에는 "전체 공지" 기능이 **없다**. 가장 가까운 것은 "오피스 전체" 방(멘션해야만 답함)과 그룹 방(멘션 없으면 멤버 전원이 답함).
3. deskrpg에서 `@이름`은 사용자가 타이핑한 문자열이 아니라 **칩**이고, 전송 순간에만 `@[이름]` 문자열이 된다. 서버는 정확히 일치하는 표시 이름만 해석한다.
4. NPC를 부르는 이동은 서버가 걷는 것이 아니라 **브라우저가 계산**한다(서버는 phase · 소유자만 기록). 우리는 Rust 서버가 권위이므로 상태 기계만 가져온다.
5. "출근(commute) 연출"은 오피스 안 NPC 출근이 아니라 **로그인 화면의 출근길 도시 장면**이다(`app/auth/AuthPageClient.tsx:12-14`, `components/CommuteCityScene.tsx`).

---

## A. 내 캐릭터

### deskrpg
- 생성/외형: `app/characters/MyCharacterForm.tsx` — 이름(`maxLength=50`, :176) + 소개(`BIO_MAX_LENGTH=2000`, `lib/my-character-limits.ts:6`) + 룩 선택(룩북). 외형은 `{officeLookId, bodyType}` 두 키만 정규형(`game/three/office-appearance.ts:3-9`). 룩은 `office-looks.ts`(10) + `office-looks-extended.ts`(40) 정의(grep 계수, 약 50종). 기본 룩 `office-jun`, 여성 기본 `office-nari`(:52-54).
- 캐릭터는 계정당 하나(`GET /api/characters/me`, `GamePageClient.tsx:289`). 접속 시 서버가 "내 캐릭터"를 확정하고 없으면 입장 거부(`server/socket-handlers.ts:1393-1410`).
- 시작 위치(`socket-handlers.ts:1440-1500`): 1순위 재접속 복원 상태(`playerResumeStates`), 2순위 `channel_members.lastX/lastY`(저장된 마지막 위치), 3순위 맵의 `spawn`(타일 중심 `(col+0.5)*32`). 그 뒤 `closestValidUnoccupiedSpawn`으로 다른 사람 · NPC와 겹치지 않는 가장 가까운 빈 칸으로 보정. 서버가 `player:spawn`을 보내기 전에는 클라이언트가 이동 패킷을 보내지 않는다(`office-simulation.ts:1890-1907`, `canMovePlayer` :1560 근처).
- 이름표: DOM 라벨(`three/office-renderer.ts:1424-1465`, 캔버스 위 `.office-actor-labels` 오버레이). 내 것은 **짙은 초록 배경 `#4e745b` + 흰 글자**, 다른 캐릭터/NPC는 `#fcfcf4eb` 배경 + `#344b3c` 글자, 12px/600, 테두리 4px 반경, 최대 폭 150px 말줄임(`three/office.css:215-229, 337-342`). 몸 색: 내 것 `#668d72`, NPC는 id 해시로 4색(`office-renderer.ts:1425-1431`).
- 서버에 위치 알림: 소켓 `player:move {x, y, direction, animation, motion}`(`office-simulation.ts:2208`). 최대 66ms에 1번(`MOVE_SEND_INTERVAL=66`, `simulation/constants.ts:6`), 0.5px 미만 + 방향/애니 동일이면 생략(:2189-2205). 서버는 `Number.isFinite`만 검사하고 그대로 저장 후 같은 맵에 `player:moved` 중계(`socket-handlers.ts:1590-1612`) — **클라이언트가 위치 권위**. 남의 캐릭터는 목표 좌표로 프레임마다 20%씩 보간, 200px 넘게 벌어지면 순간이동, 0.5px 미만이면 스냅(`simulation/remote-player.ts:41-52`, `LERP_FACTOR=0.2`).
- 좌표계: 서버 픽셀 1타일=32px(`constants.ts:2`), 맵 40x30. 렌더러는 1타일=월드 1단위(`pixelToWorld`)로 쓴다.

### WorkAdventure
- 이름표는 Phaser DOM 레이어(`front/Phaser/Components/UsernameDisplay.ts`): 글자 10px(:12), 배경 `rgba(27,42,65,0.5)`(:15), 흰 글자, 높이 14 게임 px(`UsernameDisplaySizes.ts:9`). 가용 상태 점(`silent` 구역이면 빨강)과 손들기 · 메가폰 아이콘이 알약 안에 붙는다. 배경색은 구역/외곽선 색 스토어로 바뀐다(`Character.ts:197-205`).
### SkyOffice
- 이름은 컨테이너 안 Text(Arial 12px 검정)이고 몸에서 30px 위(`characters/Player.ts:41-54`). 내 캐릭터에 별도 색 구분 없음.

## B. 키보드 · 마우스 이동

| 항목 | deskrpg | WorkAdventure | SkyOffice |
|---|---|---|---|
| 키 | **방향키만**(`office-simulation.ts:2965-2968`). 가로채는 키: 방향키 · 스페이스(스크롤 방지) · `/`(Firefox 빠른 검색 방지) (:109, :1689) | WASD · ZQSD · 방향키, Shift=가속, Space=상호작용, F=따라가기(`UserInputManager.ts:97-147`) | WASD + 방향키, E=앉기/일어나기, R=사물 사용, Enter=채팅 열기, Esc=닫기(`Game.ts:43-57`) |
| 속도 | `PLAYER_SPEED=120` px/s = 3.75 타일/s (`constants.ts:5`) | `WOKA_SPEED` 기본 9(`docs/.../env-variables.md:113`) × 20 = **180 px/s**(`Player.ts:140`), Shift 2.5배=450 px/s(`Player.ts:120`) | 200 px/s(`MyPlayer.ts:129`) |
| 대각선 | 각 축 속도에 `Math.SQRT1_2`(`office-simulation.ts:3119-3122`) | 별도 정규화 코드를 못 찾음(Phaser 물리가 하는지는 미확인) | `velocity.setLength(speed)`로 정규화(`MyPlayer.ts:157`) |
| 충돌 | 키 이동: 진행 방향 ±12px 앞 타일이 걷기 가능인지 검사(:3100-3117) + 선분 검사 `clearMovementSegment` + 다른 액터와 `clearActors`(:3128-3146), 몸 반지름 0.22타일(`navigation.ts:3`). 막히면 그 프레임 이동 0 | Phaser Arcade 충돌 | `physics.add.collider`로 땅 레이어/자판기(`Game.ts:144-145`), 몸통 박스는 스프라이트의 50% x 20% |
| 클릭 이동 | 왼쪽 클릭=걷기, **A\* 8방향**(4096회 반복 상한, 대각선은 양옆이 모두 걷기 가능해야 허용, 경로를 직선 구간으로 단순화) (`navigation.ts:79-140`). 경로 따라가기는 `dt=min(delta,100ms)`로 `traffic.step`(:3060-3072), 타일 중심 2px 이내 도착, 정체 감지(`pathStuckTimer`). 키를 누르면 경로 취소(:2976-2979) | 탭/클릭으로 경로 이동, 도착점 마커 표시(`Player.ts:93-105`) | 클릭 이동 없음 |
| 클릭 vs 드래그 | 포인터 이동 5px 이상이면 드래그=클릭 아님(`three/pointer-gesture.ts:12-26`). 왼쪽 드래그=화면 이동, 오른쪽 드래그=회전, 휠=확대/축소(`office-renderer.ts:284-286`) | — | — |
| 채팅 입력 중 키 | `isTypingTarget`: INPUT/TEXTAREA/SELECT/contentEditable이 포커스면 keydown 무시(:100-107, :1687). 멘션 에디터도 keydown에서 `stopPropagation`(`MentionEditor.tsx:366`). 창 blur 때 눌린 키 전부 해제(:1704-1708) | `enableUserInputsStore`가 메뉴/채팅/폼/이모트/모달 포커스 스토어를 합성해 키 입력 비활성(`Stores/UserInputStore.ts:13-30`) | Enter로 채팅 포커스, 전송/Esc 시 `blur()`로 게임에 돌려줌(`Chat.tsx:180-199`), `disableGlobalCapture`(`Game.ts:52`) |
| 카메라 따라가기 | 매 프레임 `(플레이어 - 타깃) * 0.08`만큼 타깃과 카메라를 같이 이동(오프셋 유지, 지연 있음, 프레임률 의존)(`office-renderer.ts:1528-1534`). 사용자가 드래그/줌/회전/각도 변경하면 따라가기 해제(`stopFollowing` :573), `따라가기` 버튼이나 땅 클릭(`focus()`, :399, :811)으로 복귀 | `startFollow(target, true, 1, 1)` = 지연 없음(`CameraManager.ts:260`) | `startFollow(player, true)` + 줌 1.5(`Game.ts:141-142`) |
| 걷기/서기 전환 | 이동 중 `animation:"walk"`, 정지 `"idle"`로 전송(:3069-3075, 3140-3150). 모델은 `walking`과 `seated`로 포즈 결정, 앉으면 다리 -90도(`three/office-actor.ts:368-413`). 방향은 4방향(큰 축 기준). 모델 회전은 `turnToward` 지수 보간 `1-exp(-14*dt)`(`navigation.ts:142-145`) | `moveBy`가 방향 갱신 + 걷기 애니(`Player.ts:204-229`) | `run_*` ↔ `idle_*` 애니 키 교체, 정지 시 서버에 1회 전송(`MyPlayer.ts:162-182`) |

프레임 규칙(deskrpg): 시뮬레이션은 한 프레임 최대 200ms로 자르고(`MAX_FRAME_DELTA_MS`, `tick-loop.ts:14`), 숨겨진 탭은 250ms 틱 + 최대 2초 따라잡기(`:16-18`). 키 이동의 `dt`는 `min(delta,100ms)`.

## C. 말 걸기

deskrpg:
- 근접 거리: `NPC_INTERACT_RADIUS = 64`px(2타일), **엄격히 `<`**(`constants.ts:8`, 판정 `office-simulation.ts:2256`). 가까운 순으로 정렬(:2258). 다른 접속자도 같은 반경으로 "근처 사람"에 들어간다(:2272).
- 안내: 반경 안에 들어오면 하단 토스트 `"{이름}을(를) 클릭하거나 /를 눌러 대화"`(`ko.ts:1960`, 키 `game.pressToTalk`; 발행 `office-simulation.ts:2307-2317`). 같은 대상 이름이면 중복 발행 안 함. 누구도 근처에 없으면 `toast:hide`. 토스트는 근접용이면 사라질 때까지 유지, 일반 알림 토스트는 4초(`GamePageClient.tsx:665`).
- 자동 인사: 반경 안에 처음 들어오고(대화창 닫힘, NPC가 idle일 때) 한 번만 `npc:auto-greet`(:2262-2267) → 말풍선 `"안녕하세요! 오늘도 반가워요."` 4.5초(`GamePageClient.tsx:1337-1344`, `ko.ts:2068`) + 알림 `"{이름} 님이 인사했습니다!"`.
- 열기: (1) NPC 클릭(`decideNpcClick`: 이미 닿는 거리/경로 길이 ≤1 → 즉시 대화, 아니면 걸어가서 도착 시 대화; `game/npc-click-intent.ts`), 걸어갈 때는 NPC 4방향 이웃 타일 중 걷기 가능+비점유 칸을 목표로(`approachNpcAndInteract`, :2321-2366). (2) `/` 키(`event.code==="Slash"`): 근처 후보가 1이면 바로 열고, 2 이상이면 "누구와 대화?" 선택 목록(`game.whoToTalkTo`)을 띄움, 후보가 없으면 아무 일 없음(:2510-2540, `GamePageClient.tsx:3981-4000`). 사람이면 `player:chat-open`.
- NPC가 나를 바라보나: **근접만으로는 안 바라본다.** 부름으로 도착했을 때만 큰 축 기준 4방향으로 나를 향한다(`game/simulation/npc-controller.ts` 도착 판정부, 거리 `TILE_SIZE+4=36px`).
- 1:1 대화창 조건: 열림 = `npc:interact` 수신(`GamePageClient.tsx:1290-1302`) → 기록 요청 `npc:history`. 자동 닫힘: 반경 안에 NPC/사람이 없고 NPC가 나에게 오는 중이 아닐 때(`office-simulation.ts:2281-2290`). 도착(부름) 시 자동 열기는 `autoOpenDialogOnArrival`이 결정 — 지도 채팅으로 부른 경우(`map-chat`)나 방 보기 활성이면 열지 않음(`GamePageClient.tsx:1380-1405`, `app/game/map-chat-walkers.ts`).
- 채팅 입력 가능 조건: 근처에 NPC 또는 사람이 있을 때만 채널 채팅 입력 활성(`chat:input-enabled`, :2293-2300). NPC가 맵 채팅에서 `@[이름]`으로 불릴 수 있어 "사람 없이 NPC만 근처"여도 열린다.

WorkAdventure: **근접 대화 버블**은 서버가 만든다. `MINIMUM_DISTANCE` 64px(두 사용자가 처음 묶이는 거리), `GROUP_RADIUS` 48px(이미 있는 그룹에 합류하는 거리), `MAX_PER_GROUP` 기본 4명(`back/src/Enum/EnvironmentVariableValidator.ts:13-17, 64`, `docs/others/self-hosting/env-variables.md:91,127,154`). 그룹/사용자 탐색은 `GameRoom.searchClosestAvailableUserOrGroup`(`back/src/Model/GameRoom.ts:600-650`): 이미 그룹이 있거나 `silent`이거나 가득 찬/잠긴 그룹은 제외, 걷는 중인 사용자는 정지할 때 합류(`Group.ts:143-160`). 거리 `>GROUP_RADIUS`이면 이탈(`GameRoom.ts:506,525`). `silent` 구역(조용한 구역)에 들어가면 이름표 상태 점이 빨강, 누구와도 말할 수 없음(`docs/map-building/inline-editor/area-editor/silent.md`).

SkyOffice: 사용자 앞 32px에 "선택자" 존이 따라다니며(이동 방향 기준 좌/우/위/아래 32px, `PlayerSelector.ts:37-45`) 겹친 사물에 `Press E to sit`(`items/Chair.ts:14`) / `Press R to use computer`(`Computer.ts:30`) 말풍선이 뜬다. 선택자와 겹침이 끝나면 안내를 지운다(`PlayerSelector.ts:49-54`). **근접 화상**: 두 플레이어 스프라이트의 물리 겹침(`overlap`)이면 `makeCall`, 조건은 연결 안 됨 + 버퍼 750ms + 양쪽 준비됨 + `myPlayerId > playerId`(한쪽만 발신)(`OtherPlayer.ts:32-45`). 겹침이 끝난 뒤 750ms 지나면 연결 해제(:155-167, 지정 구역 예외).

## D. 멘션

### 입력 형식과 직렬화 (deskrpg)
- 화면: `contentEditable` 에디터(`components/mention-input/MentionEditor.tsx`). 내용은 [텍스트 조각 / 멘션 칩 / 스킬 칩] 시퀀스(`mention-model.ts:19-22`). 칩은 `contenteditable=false`, 표시 `@이름`, 클래스 `rounded px-1.5 py-0.5 text-sm font-semibold` + 강조색(일반 채팅 `bg-npc/15`, 회의 `bg-meeting/15`; `components/chat-accent.ts:28,34`)(MentionEditor.tsx:115-122).
- 전송 순간에만 `@[이름]` 문자열로 직렬화(`mention-model.ts:25-30`). 이름 속 `\ [ ]`는 역슬래시로 이스케이프(`lib/conversation/mention.ts:34-36`). 서버 파서: `@\[((?:\\.|[^\]\\])*)\]`(:44). 접미 조사(`@단비는`)와 부분 일치 오인을 피하려고 **정확 일치 + 괄호 형식**을 강제한다(:3-6 주석).
- `@` 트리거: 줄 시작이거나 직전이 공백일 때만, 쿼리 안에 공백이 들어오면 닫힘(이메일 `a@b` 제외)(`mention-model.ts:55-75`). 필터: 소문자화한 **부분 포함**(`includes`) (:77-84). 결과 없음 문구 `chat.mentionNoMatch`="해당하는 직원이 없습니다"(`ko.ts:2153`).
- 목록 키: ↓/↑ 순환 이동, Enter/Tab 선택(후보 0개면 무동작), Esc 닫기(`mention-model.ts:95-107`, 핸들러 `MentionEditor.tsx:364-396`). Enter=전송, Shift+Enter=줄바꿈 텍스트 삽입(:383-392). IME 조합 중에는 키 처리 안 함(`isComposing`, :368). 칩 바로 뒤에서 Backspace는 칩을 통째로 삭제(:397-413). 선택 시 칩 + 공백을 넣고 커서를 공백 뒤로(:303-330). 목록은 입력창 위(`bottom-full`), 폭 14rem, 최대 높이 12rem.
- 스킬(`/이름`) 칩도 같은 틀이나 텍스트로는 안 나가고 `skills[]`로 별도 전송. 스킬 사용은 멘션이 정확히 1명일 때만(`room-socket.ts:355-356`).
- 메시지 길이 500자 제한(`server/room-socket.ts:47`, :330 `slice`).

### 누가 답하나
- 방 정책 `ReplyPolicy = "mention" | "members"`(`lib/chat-rooms-policy.ts:1`). `decideResponders`(:179-189): **mention(오피스 전체 방)** = 멘션된 멤버만 답, 멘션 없으면 **아무도 답하지 않음**; **members(그룹 방)** = 멘션 없으면 멤버 전원, 멘션 있으면 그 부분집합만.
- 여러 명 멘션: 등장 순서대로, 중복 제거, 화자 본인 제외(`mention.ts:113-131`). 사람이 말하면 멘션된 모두가 답해야 하므로 NPC 간 연쇄 예산을 리셋(`ChatQuota` 기본 6, 사람이 말하면 `resetByHuman`; `lib/conversation/chat-quota.ts:9-24`). NPC는 각자 큐에서 한 턴씩(`open-chat-runtime.ts` dispatch).
- 멘션했는데 아무도 매칭 안 되면(`targets.length===0` 이고 멘션 이름이 1개 이상): **보낸 사람에게만** `room:mention-skipped {reason:"no_match"}` → `"지목한 상대가 이 방에 없어 아무도 답하지 않았습니다."`(`open-chat-runtime.ts:219-222`, `room-runtime.ts:396-399`, `ko.ts:2134`). 멘션 자체가 없으면 안내도 없음(조용).
- 응답 대기 한도: 턴당 무응답 180초 / 최대 600초(`room-runtime.ts:283`), 큐가 가득 차면 `room:npc-aborted {reason:"queue_full"}`.

### 멘션된 NPC의 몸짓
- 멘션된 NPC의 답변 턴이 **시작되는 순간**(생각 중 상태와 동시) 서버가 `npc:come-to-player {targetPlayerId, reason:"map-chat", roomId}`를 채널에 방송 → NPC가 나에게 걸어옴(`room-runtime.ts:304-316`). 대화창은 **열지 않는다**(답이 채팅 패널에 나오므로) (`GamePageClient.tsx:1380-1405`, `map-chat-walkers.ts`). 그 방을 보는 동안은 대기하고, 방을 바꾸면 타이머 없이 즉시 복귀(`game/npc-auto-return.ts:22-33`).

## E. 전체 공지 / 방송

| 저장소 | 기능 | 받는 방식 |
|---|---|---|
| deskrpg | **없음.** 가장 가까운 것 3가지: (1) "오피스 전체" 방(`room.office`, kind `office`, 정책 mention) — 채널 전원이 보지만 NPC는 멘션해야 답함. 자동화 알림(칸반 완료/차단/검토, 크론 결과, 승인 요청)이 이 방에 **알림 카드**로 올라옴(`lib/chat-rooms-policy.ts:34-117`). (2) 그룹 방(정책 members) — 멘션 없이 보내면 멤버 NPC **전원이 각자 턴으로** 답함 = "모두에게 지시"에 가장 가깝다. (3) 회의(`meeting:user-speak`, `meeting-discussion.ts:1028`) — 참석 NPC가 토론에 사용자 발언을 반영 | 방 런타임이 `decideResponders` 결과 NPC마다 큐에 한 턴씩 넣음. 사람이 말하면 연쇄 예산 6 리셋 |
| WorkAdventure | **글로벌 메시지** `PlayGlobalMessage {type, content, broadcastToWorld, joinUrl}`(`messages/protos/messages.proto:685-691`). **admin 태그가 있는 사용자만** 보낼 수 있고(`pusher/services/SocketManager.ts:1196-1204`), `broadcastToWorld=true`면 같은 월드의 모든 방으로, 아니면 현재 방으로 `AdminRoomMessage`(보낸 사람 이름 · 아바타 포함)를 보냄(:1209-1235). `joinUrl`은 같은 도메인 한정으로 "이 구역으로 이동" 링크 | 수신 화면 표시 방식은 이번에 확인하지 못함. 서버 쪽 `sendGlobalAdminMessage` RPC는 "미구현, 호출하면 throw"라고 주석(`services.proto:20`). 비슷한 별개 기능: 연단(podium)/청중 구역의 메가폰 방송(`docs/map-building/inline-editor/area-editor/broadcast.md`), 구역 `silent` |
| SkyOffice | **없음.** 방 단위 전체 채팅만(서버 최근 100개 유지, `ChatMessageUpdateCommand.ts:21-23`), 말풍선은 다른 사람 머리 위에 6초/최대 70자(`Player.ts:64-100`), 입력 Enter, 내 머리에도 반영(`Chat.tsx:205`) | — |

## F. 부르기 · 보고 (deskrpg)

상태(서버 `npc.phase`): `idle`(자리) → `called` → `waiting`(내 옆) → `returning` → `idle`; 별도 `ambient`(산책)(`server/npc-coordination.ts:635-700, 780-830`). 클라이언트 `moveState`: idle / moving-to-player / waiting / returning / strolling(`npc-auto-return.ts:11-14`).

흐름:
1. 시작점 3가지: 직원 목록/우클릭 메뉴 `호출`(`ko.ts:2190`), 멘션(D), 자동화 보고(아래). 소켓 `npc:call {npcId, reason?, roomId?}`.
2. 서버 검사: 없음 `unknown_npc` / 회의 예약 `meeting_reserved` / 다른 사람이 소유 중 `already_claimed`(단 `ambient` 산책 중이거나, 사람이 누른 호출의 `returning`은 가로채기 허용; 멘션 호출은 복귀 중 NPC를 안 뺏는다) (`npc-coordination.ts:635-690`). 통과하면 `ownerSocketId=나`, `phase=called`, `npc:come-to-player {targetPlayerId: socket.id, reason?}` 방송. 같은 사람이 재호출해도 다시 걷게 한다.
3. 걷기(소유자 브라우저): 이미 **36px(`TILE_SIZE+4`) 안**이면 걷지 않고 바로 waiting. 아니면 내 타일로 A\*, 속도 **summon = 300 px/s**(기본 걷기 150의 2배, 뛰어옴; 스프라이트는 225 px/s 이상이면 달리기)(`lib/npc-motion-config.ts:29-34, RUN_SPEED_THRESHOLD`), 채널 설정으로 20~480 px/s(5 단위) 조절. 10타일 거리면 약 1.1초.
4. 도착: 36px 안에서 정지 → 나를 향해 큰 축 기준 4방향으로 서고 waiting, `npc:position-update` + `npc:arrived` 전송 → 서버 phase `waiting`. 화면: 도착 말풍선(`arrivalBubbleText`) · 토스트 `pressToTalk` · `npc:movement-arrived` (`office-simulation.ts:1500-1545`).
5. 보고/대화: 대화창이 자동으로 열리고(지도 채팅 호출 제외) 보고로 왔으면 대화창 상단에 그 보고를 표시(`GamePageClient.tsx:1380-1405`). 일하는 중인 NPC도 막지 않고 부르며 `"{이름} 님은 지금 {n}건을 처리하는 중입니다. 작업은 계속됩니다."` 토스트(`ko.ts:1957`).
6. 돌아가기: 대화창이 닫혀 있고(`dialogOpen=false`) 부름이 "지금 보는 방" 때문이 아니면 **10초**(`waitDurationMs=10000`, `npc-controller.ts:82`, 판정 `office-simulation.ts:2780-2796`) 후 `npc:return-home` → phase `returning`, 걷기 속도 **150 px/s**로 자리 복귀, 집 2px 안에서 `npc:arrived` → `idle`. 자리까지 못 가는 경우 `ambient`.
7. 자동화 보고: **서버가 NPC를 움직이지 않는다.** 오피스 방에 보고 알림(`card_review/card_blocked/card_done/cron_failed`)이 오면 그걸 받은 **브라우저**가 기존 `npc:call {reason:"report"}`를 스스로 쏜다(`game/report-queue.ts:1-9`, `app/game/GamePageClient.tsx:2439-2475`). 대화창 · 칸반 · 크론 창이 열려 있으면 끼어들지 않고 대기, 같은 보고를 두 번 부르지 않음(`sent/rejected/dismissed`), 사용자가 확인 없이 닫은 보고는 약 10분 뒤 재소환 후보(`DISMISSED_REPORT_REVIVE_MS`, `npc-report-dispatch.ts:65`), 목록 `남은 보고`/`다시 부르기`(`ko.ts:3126-3130`).
8. 서버는 소유권 1명 규칙을 강하게 지킨다(없으면 모든 접속 브라우저가 같은 NPC를 각자 걷게 한다). 이 점은 우리와 무관: Rust 서버가 걷기를 직접 계산하므로 소유자 개념이 필요 없다.

## G. 게임 느낌을 만드는 요소 (있는 것 / 없는 것)

deskrpg 있음:
- 화면 아래 조작 안내(오른쪽 아래, `bottom:20px right:24px`, 11px, 포인터 무시): `"클릭: 걷기 · 드래그: 화면 이동 · 우클릭 드래그: 회전 · 휠: 확대/축소"`(`ko.ts:3899-3900`, `ThreeGame.tsx:342-344`, `office.css:286-293`). 회의 모드에서는 `"드래그: 회전 · 이동과 좌석 변경은 오피스로 돌아간 뒤 가능합니다."`
- 카메라 도구 줄(`office-camera-tools`, 오른쪽 `20px`/아래 `65px`, 32px 버튼 간격 3px, 흰빛 배경): 순서 = 전체 보기(`Maximize`) · 왼쪽 회전 · 오른쪽 회전 · 입체 시점 · 위에서 보기 · 내 캐릭터 따라가기(`Focus`) · 확대 `+` · 축소 `-`(`ThreeGame.tsx:262-341`). 동작: 회전 = 타깃 기준 ±45도(`rotateCamera`, `office-renderer.ts:457-465`), 확대 ×0.8 / 축소 ×1.25 거리 8~(최대 85 이상)로 클램프(:467-475), 입체 = 극각 `π/3.5`, 위에서 = 0.15 rad(:447-455), 전체 보기 = 맵 중앙 + 종횡비 기반 거리(`overview`, :425-445). 초기 카메라 위치 (22,22,28), 타깃 (15,0,12), `minDistance 8`, `maxDistance 85`, 극각 0.15~0.46π, 감쇠 사용(:275-286).
- 호버/선택: 바닥 클릭 위치 커서 `#578467`(막힌 곳 `#bd6756`)는 배치/스폰 편집 때만 표시, 가구 하이라이트와 좌석 호버, 칸반판 클릭 시 도착해서 칸반 열기(`office-renderer.ts:651-760`).
- 앉기: 좌석 위를 호버하면 후보 좌석 선택 · 클릭하면 서버 좌석 예약 → 경로로 걸어가 **좌석 중심에 도착**해야 앉음(`office-simulation.ts:2982-3000, 3030-3040`; 도착 못 한 좌석 예약은 만료 시 삭제 `npc-coordination.ts:413-418`, 예약 시 만료 시각 +60초 `:890`). 키 이동을 시작하면 자리에서 일어남(:2965-2975). 좌석 선택 팝업/좌석 번호 배지 `office-seat-badge`.
- 말풍선(머리 위 DOM, 최대 폭 240px, 3줄 말줄임; `office.css:230-251, 343-353`): 채팅 응답 미리보기 12초/240자(`speech-previews.ts`), 인사 4.5초, 렌더러 `talk()` 4초(`office-renderer.ts:477`).
- NPC 산책(ambient: 걷기 55 px/s, 잡담 간격 7.5초 `npc-smalltalk.ts:93`), 일하는 중 표시, 회의 카메라, 알림 토스트(4초), 컨텍스트 메뉴, 성능 HUD.
- 출근 연출: 로그인 화면의 출근길 도시(차량 · 가로수 · 걷는 캐릭터 · 해와 구름). 오피스 안 연출 아님(`app/auth/AuthPageClient.tsx:12-14`, `game/three/commute-*.ts`).
- 없음: 효과음(오디오 사용 코드 없음), 미니맵(없음), 달리기 키, 이모트.

WA: 이모트 메뉴, 손들기, 말풍선 "말하기" 5초/생각 구름 무기한(`SayPopUp.svelte:99-103`), 지도 탐색기, 따라가기(F), 월드 사운드(`SoundManager.ts`). SkyOffice: 앉기 단축키 E/R, 모바일 가상 조이스틱, 화이트보드/컴퓨터 화면공유, 자판기.

## H. 우리에게 권하는 상위 10 (우선순위 순)

단위 환산: deskrpg 1타일=32px=월드 1단위. 우리 월드 단위가 다르면 비율로 바꾼다.

1. **키보드 이동 + 입력 중 가드** — 화면만. 근거: deskrpg(방향키만), WA(WASD+ZQSD, Shift 2.5배), Sky(대각선 정규화). 수치: 3.75 타일/s(120px/s), 대각선 `×1/√2`, 프레임 `dt` 최대 100ms, 몸 반지름 0.22타일. 우리: 방향키 + WASD 둘 다, `isTypingTarget`(INPUT/TEXTAREA/SELECT/contentEditable)이면 무시, 창 blur 시 키 상태 비움, 채팅 에디터 keydown은 `stopPropagation`. 달리기는 WA 방식 Shift ×2.5를 선택 사양으로(원본 deskrpg엔 없음 → 사용자 결정 필요).
2. **근접 안내 + `/` 키 + 클릭으로 말 걸기** — 화면만(1:1 창 열기는 기존 흐름). 근거: deskrpg. 수치: 반경 64px=2타일(`<`), 안내 문구 `"{이름}을(를) 클릭하거나 /를 눌러 대화"`, 후보 2명 이상이면 "누구와 대화?" 목록, 후보 0이면 무반응, 반경 이탈 시 대화창 자동 닫힘(단 NPC가 걸어오는 중이면 유지), 첫 접근 시 1회 인사 말풍선 4.5초. 우리: 매 프레임 거리 계산 + 토스트 하단 고정.
3. **멘션 칩 에디터와 `@[이름]` 직렬화** — 에디터는 화면만, 응답자 결정은 **서버 필요**. 근거: deskrpg `MentionEditor`/`mention.ts`. 수치: `@`는 줄 시작/공백 뒤에서만, 부분 포함 필터, ↑↓ 순환/Enter·Tab 선택/Esc 닫기, IME 조합 중 무시, 칩 Backspace 통삭제, 칩 색 `bg-npc/15`, 전송 시 `@[이름]`(`\ [ ]` 이스케이프), 길이 500자. 서버(Rust): 정규식 `@\[((?:\\.|[^\]\\])*)\]`, 표시 이름 정확 일치, 중복 제거, 화자 제외.
4. **부르기 상태 기계(called→waiting→returning→idle)** — **서버 필요**(우리는 서버가 이동 계산). 근거: deskrpg `npc:call`/`npc:arrived`/`npc:return-home`. 수치: 도착 판정 36px(1.125타일), 호출 속도 300 px/s(=9.4 타일/s), 복귀 150 px/s, 대화 없을 때 10초 후 복귀, 이미 36px 안이면 즉시 waiting, 도착 시 나를 향해 4방향 정렬. 오류 코드 `unknown_npc/meeting_reserved/already_claimed`, 재호출 허용, 일하는 중이어도 호출 가능 + 토스트.
5. **멘션되면 담당이 걸어와서 답하고, 그 방을 보는 동안 곁에 남는다** — **서버 필요**(답변 턴 시작 시 come 이벤트). 근거: deskrpg `room-runtime.ts` + `npc-auto-return.ts`. 수치: 턴 시작과 동시에 이동 시작(생각 중), 1:1 창은 열지 않음(답이 채팅 패널에 뜸), 방 전환 시 즉시 복귀, 직접 호출은 10초 타이머. 멘션 없는 오피스 방은 무답 + 매칭 실패 시에만 보낸 사람에게 안내.
6. **클릭 이동(A\*) + NPC 클릭 = 걸어가서 대화** — 화면만. 근거: deskrpg `navigation.ts`, `npc-click-intent.ts`, `pointer-gesture.ts`. 수치: 8방향, 반복 4096회 상한, 직선 구간 단순화, 드래그 5px 이상이면 클릭 아님, 이미 닿는 거리면 즉시 대화(`interact-now`), 키 입력이 경로를 취소, 목표는 NPC의 4방향 이웃 중 비점유 걷기 가능 칸.
7. **카메라 따라가기 + 도구 줄 + 조작 안내 바** — 화면만. 근거: deskrpg. 수치: 프레임당 보간 0.08(프레임률 의존이므로 우리는 `1-exp(-k·dt)` 형태로 바꾸는 것을 권함, 지연 약 8프레임 = 0.13초 체감), 오프셋 유지, 사용자가 드래그/줌/회전하면 따라가기 해제, 8개 버튼 순서(전체/좌회전/우회전/입체/위/따라가기/+/-), 회전 ±45도, 줌 ×0.8/×1.25, 거리 8~85, 안내 문구 `"클릭: 걷기 · 드래그: 화면 이동 · 우클릭 드래그: 회전 · 휠: 확대/축소"`.
8. **이름표와 머리 위 말풍선** — 화면만. 근거: deskrpg(내 이름표 `#4e745b` 흰 글자 vs 다른 `#fcfcf4eb`/`#344b3c`, 12px/600, 최대 150px 말줄임), SkyOffice(말풍선 6초/70자/폭 165), WA(말하기 5초, 상태 점). 수치: 말풍선 최대 폭 240px, 3줄 클램프, 응답 미리보기 12초/240자(`…` 237자+`...`). 우리: 6명 + 나 정도라 DOM 오버레이로 충분, 겹침 방지 `label-layout` 참고.
9. **"팀 방" 정책: 멘션 없으면 전원 / 멘션하면 부분집합** — **서버 필요**. 근거: deskrpg `decideResponders`(members 정책 = 가장 가까운 "모두에게 지시"), WA 글로벌 메시지(admin 전용, `broadcastToWorld` 플래그, 보낸 사람 표시)는 참고만. 주의: 원본에 별도 "전체 공지" 기능은 없으므로 "공지"를 새로 만들려면 스펙 결정이 필요하다(AGENTS.md 9번, 원본과 다르게 만들 까닭이 있어야 함). 수치: NPC 간 연쇄 예산 6(사람이 말하면 리셋), 응답은 NPC마다 개별 큐, 턴 무응답 180초/최대 600초.
10. **내 캐릭터 생성과 시작 위치 복원 + 앉기** — 서버 필요(마지막 위치 저장), 앉기는 화면만. 근거: deskrpg(이름 50자 · 소개 2000자 · 룩 약 50종, 위치 복원 우선순위 재접속→저장 위치→스폰 + 겹침 보정), SkyOffice(E로 앉기/일어나기, "Press E to leave"). 수치: 위치 알림 66ms 이하 간격/0.5px 변화 임계(우리는 NPC 부르기 때 필요한 거리 판단용이면 서버에 "정지 시 + 이동 중 66ms" 정도면 충분), 앉기는 좌석 중심 도착 후만 앉는 포즈, 키 이동 시작하면 일어남.

서버가 필요한 것: 3(응답자 결정) · 4 · 5 · 9 · 10(위치 저장). 화면만으로 되는 것: 1 · 2 · 6 · 7 · 8 · 앉기 연출. (2는 서버 기존 1:1 대화 연결이 있으면 화면만.)

---

## 확인 못 한 것
- WA 글로벌 메시지가 **수신 화면에 어떻게 보이는지**(배너/타이머)와 `sendGlobalAdminMessage`의 실제 사용처. WA 대각선 속도 정규화(Phaser 물리 쪽일 수 있음).
- deskrpg 우클릭 메뉴의 정확한 항목 목록과 보고 대화창 UI 상세, `autoOpenDialogOnArrival` 결정표(함수 본문은 안 읽음).
- deskrpg 카메라 보간 0.08은 프레임당 값이라 실제 지연 시간은 주사율에 따라 달라진다(60fps 가정 시 위 수치).
- 실행하지 않았으므로 화면에서 실제로 어떻게 보이는지(애니메이션 속도, 걷는 보폭 일치)는 코드 수치로만 확인.
- SkyOffice 서버의 위치 검증 여부(코드 구조상 클라이언트 신뢰로 보이나 전체를 읽진 않음).
- 라이선스는 파일 머리말 수준으로만 확인(WA 전체 NOTICE 미검토).

## 부록: rea 6.7.0 로 한 것 / 못 한 것

| 작업 | 명령 | 결과 | 쓸모 |
|---|---|---|---|
| 환경 확인 | `rea doctor`, `rea --help`, `rea capabilities` | 정상 동작 확인, 명령 목록 확보 | 명령 선택 |
| 소스 트리 가져오기 | `rea import-reference-source <deskrpg/src> --detail summary` | 4.6초, 항목 1754개(파일 1521, 폴더 233), 관계 6969, 파싱 실패 0, 언어 TS/TSX/JS/CSS/JSON, git head `5d80963` 확인(`inventory_state: partial`) | 파일 구조와 규모 파악, 커밋 일치 확인. 동작 수치는 못 줌 |
| JS 앱 정적 분석 | `rea analyze-javascript-application <deskrpg/src>` | 70초, 240MB JSON. JS 파일 1499개 파싱, AST 노드 131만, 모듈 그래프 9780 노드/11330 간선, 엔드포인트 183 | 아래 한계 때문에 보조용 |
| 이벤트 리스너 / 타이머 추출 | `python3 -I`로 semantic_graph 노드 중 `listener`/`event`/`timer`만 필터 | 의미 노드 상한(10만) 때문에 **245개 모듈만** 분석됨. 추출된 리스너 122 · 이벤트 120 · 타이머 7 이 거의 `GamePageClient.tsx`(EventBus 구독) 에 몰림 | `GamePageClient`의 EventBus 구독 위치 확인에만 유용. `office-simulation.ts`의 keydown 리스너, `office-renderer.ts`의 포인터 리스너, 소켓 이벤트는 **누락**(분석 범위 밖) |
| Electron 경계 | 같은 명령 | IPC · 브라우저 창 · preload 모두 0 (웹 앱이라 해당 없음) | 해당 없음 |
| 키 이름 · 속도 · 거리 · 상태 기계 · 멘션 규칙 | rea로 불가 | `grep`/`sed`/파일 읽기로 직접 확인(위 본문의 파일:줄 전부) | rea가 못 한 부분 |
| WA · SkyOffice | rea 사용 안 함 | 규모가 커서(WA) 대상 파일만 grep으로 읽음 | — |

rea 산출물(`rea-deskrpg-import.json` 93KB, `rea-deskrpg-js.json` 240MB)과 임시 스크립트는 `scratchpad/refs/`에 있다(저장소 밖). 받은 소스는 `scratchpad/refs/src3/{deskrpg,workadventure,skyoffice}/`.
