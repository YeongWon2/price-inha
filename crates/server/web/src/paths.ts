// 화면이 부르는 같은 출처 경로의 표(server 11차). 화면 코드는 이 표 · 함수의 값으로만 서버를 부른다.
// 바깥 주소는 없다. 번호가 들어가는 자리는 표에서 `{id}`로 적는다.

/**
 * 화면 이름: 설정(`/`) · 키(`/keys`) · 사무실(`/office/`) · 폰(`/phone/`),
 * 관리 화면(server 26차 4번) 계좌(`/account/`) · 템플릿(`/templates/`) · 규칙(`/rules/`) · 히스토리(`/history/`).
 */
export type ScreenName = "setup" | "keys" | "office" | "phone" | "account" | "templates" | "rules" | "history";

/** 승인 카드의 답. */
export type Verb = "approve" | "reject";

/** 화면이 열리는 주소(단추가 다른 화면으로 갈 때 쓴다). */
export const SCREEN_ROOTS: Readonly<Record<ScreenName, string>> = {
  setup: "/",
  keys: "/keys",
  office: "/office/",
  phone: "/phone/",
  // 관리 화면 넷(server 26차 4번).
  account: "/account/",
  templates: "/templates/",
  rules: "/rules/",
  history: "/history/",
};

/** 화면들이 부르는 경로 이름 → 경로. */
export const API = {
  session: "/api/session",
  socket: "/ws",
  setup: "/api/setup",
  setupKeys: "/api/setup/keys",
  setupSymbols: "/api/setup/symbols",
  setupAi: "/api/setup/ai",
  setupBackup: "/api/setup/backup",
  setupSlots: "/api/setup/slots",
  setupProviders: "/api/setup/providers",
  // 설정 화면 보강(server 31차 1번): 뜻 검색 설정 읽기 `GET` · 바꾸기 `POST`(그 컴퓨터만) · 진단 읽기 `GET`(읽기만).
  // 시장 · 담당 모델은 설정 상태([`API.setup`])의 `market` · `models` 칸에서 읽는다.
  setupSemantic: "/api/setup/semantic",
  setupDoctor: "/api/setup/doctor",
  // 알림음 켜기 · 끄기(31차 고침, 결정 22): 읽기 `GET` · 바꾸기 `POST` `{"enabled": bool}` — 서버가 둔다.
  setupSound: "/api/setup/sound",
  accessKey: "/api/access/key",
  accessUnlock: "/api/access/unlock",
  accessLog: "/api/access/log",
  remoteKeys: "/api/remote/keys",
  office: "/api/office",
  officeChat: "/api/office/chat",
  officeLooks: "/api/office/looks",
  officeLook: "/api/office/look",
  // 내 캐릭터(server 32차 1번, C17): 읽기 `GET` · 이름 · 외형 바꾸기 `POST` `{"name"?, "look"?}` ·
  // 자리 남기기 `POST` `{"x", "y", "dir"}`(띄엄띄엄 — `office/player-sync.ts`).
  officeMe: "/api/office/me",
  officeMePosition: "/api/office/me/position",
  // 전체 공지(server 33차 3번, C19): 보내기 `POST` `{"text", "sourceMessageId"}` · 지난 공지 `GET ?limit=`.
  officeNotice: "/api/office/notice",
  officeNotices: "/api/office/notices",
  // 담당 부르기(server 33차 4번 · 2번, C20): 부르기 `POST` `{"agent"}` · 도착 알림 `POST` `{"agent"}` ·
  // 지금 보는 방 알림 `POST` `{"room": "team" | "direct"}`.
  officeSummon: "/api/office/summon",
  officeSummonArrived: "/api/office/summon/arrived",
  officeSummonView: "/api/office/summon/view",
  pushKey: "/api/push/key",
  pushSubscribe: "/api/push/subscribe",
  notices: "/api/notices",
  dailySummary: "/api/daily-summary",
  approvals: "/api/approvals/",
  // 템플릿 화면(server 27차): 읽기 `GET` · 고르기 `POST` · 값 맞추기 `POST`. 멈추기는 [`templateStopPath`].
  templatesView: "/api/templates/current",
  templates: "/api/templates",
  templatesAdjust: "/api/templates/adjust",
  // 규칙 화면(server 28차): 규칙 읽기 `GET` · 개선안 목록 `GET` · 개선안 답하기 `POST`.
  rules: "/api/rules",
  proposals: "/api/proposals",
  proposalAnswer: "/api/proposals/answer",
  // 히스토리 화면(server 29차): 매매 목록 `GET ?before=&max=&state=` · 승인 요청 기록 `GET ?before=&max=&status=`.
  // 매매 하나는 [`tradePath`], 하루 정리는 [`API.dailySummary`]`?date=`.
  trades: "/api/trades",
  approvalLog: "/api/approvals",
  // 계좌 · 운용 화면(server 30차): 모의계좌 읽기 `GET` · 팀 상태 `GET` · 다시 시작 `POST`(몸통 없음).
  account: "/api/account",
  team: "/api/team",
  teamRestart: "/api/team/restart",
} as const;

/** 매매 하나 경로 `/api/trades/{id}`(server 29차 1번, 번호는 주소 글자로 감싼다). */
export function tradePath(id: number | string): string {
  return API.trades + "/" + encodeURIComponent(String(id));
}

/** 실전 템플릿 멈추기 경로 `/api/templates/{id}/stop`(server 27차 2번, 번호는 주소 글자로 감싼다). */
export function templateStopPath(id: number | string): string {
  return API.templates + "/" + encodeURIComponent(String(id)) + "/stop";
}

/**
 * 사무실 담당 외형(server 20차 4번): 읽기 `GET` · 바꾸기 `POST`. 사무실 경로([`API.office`]) 아래에 둔다.
 * (21차 6번) 정식 상수([`API.officeLooks`] · [`API.officeLook`])이고 `SCREEN_PATHS.office`에 들어 있다.
 */
export const OFFICE_LOOK_PATHS = {
  list: API.officeLooks,
  choose: API.officeLook,
} as const;

const APPROVALS = ["/api/approvals/{id}", "/api/approvals/{id}/approve", "/api/approvals/{id}/reject"] as const;

/** 각 화면이 부르는 경로(지금 화면과 같은 목록, 사무실은 승인 카드 경로가 더해진다). */
export const SCREEN_PATHS: Readonly<Record<ScreenName, readonly string[]>> = {
  setup: [
    API.session,
    API.socket,
    API.setup,
    API.setupKeys,
    API.setupSymbols,
    API.setupAi,
    API.setupBackup,
    API.setupSlots,
    API.setupProviders,
    API.accessKey,
    API.accessUnlock,
    API.accessLog,
    // server 31차 1번 (다) · (라): 뜻 검색 설정 · 진단.
    API.setupSemantic,
    API.setupDoctor,
    // 31차 고침(결정 22): 알림음 설정.
    API.setupSound,
  ],
  keys: [API.remoteKeys, API.setupSlots, API.setupProviders],
  office: [
    API.session,
    API.office,
    API.officeChat,
    API.officeLooks,
    API.officeLook,
    // server 32차 1번: 내 캐릭터 읽기 · 바꾸기 · 자리 남기기.
    API.officeMe,
    API.officeMePosition,
    // server 33차 3번 · 4번: 전체 공지 · 지난 공지 · 담당 부르기 · 도착 알림 · 보는 방 알림.
    API.officeNotice,
    API.officeNotices,
    API.officeSummon,
    API.officeSummonArrived,
    API.officeSummonView,
    API.socket,
    ...APPROVALS,
  ],
  phone: [API.pushKey, API.pushSubscribe, API.notices, API.dailySummary, ...APPROVALS],
  // 계좌 · 운용 화면(server 30차 2번 · 3번): 계좌 읽기 · 팀 상태 · 다시 시작 · 실시간 연결.
  account: [API.session, API.account, API.team, API.teamRestart, API.socket],
  // 템플릿 화면(server 27차 4번): 읽기 · 고르기 · 값 맞추기 · 멈추기, "실전 운용 시작"은 승인 카드 읽기 · 승인.
  templates: [
    API.session,
    API.templatesView,
    API.templates,
    API.templatesAdjust,
    "/api/templates/{id}/stop",
    "/api/approvals/{id}",
    "/api/approvals/{id}/approve",
  ],
  // 규칙 화면(server 28차 2번): 규칙 읽기 · 개선안 목록 · 개선안 답하기(번호 · 지문을 그대로 보낸다).
  rules: [API.session, API.rules, API.proposals, API.proposalAnswer],
  // 히스토리 화면(server 29차 2번): 매매 목록 · 매매 하나(근거가 함께 온다) · 하루 정리 · 승인 요청 기록.
  history: [API.session, API.trades, "/api/trades/{id}", API.dailySummary, API.approvalLog],
};

/** 승인 카드 읽기 경로 `/api/approvals/{id}`(폰과 같다, 번호는 주소 글자로 감싼다). */
export function approvalPath(id: number | string): string {
  return API.approvals + encodeURIComponent(String(id));
}

/** 승인 · 거절 경로 /api/approvals/{id}/approve · reject(폰과 같다). */
export function decidePath(id: number | string, verb: Verb): string {
  return approvalPath(id) + "/" + verb;
}
