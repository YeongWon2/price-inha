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
  accessKey: "/api/access/key",
  accessUnlock: "/api/access/unlock",
  accessLog: "/api/access/log",
  remoteKeys: "/api/remote/keys",
  office: "/api/office",
  officeChat: "/api/office/chat",
  officeLooks: "/api/office/looks",
  officeLook: "/api/office/look",
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
} as const;

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
  ],
  keys: [API.remoteKeys, API.setupSlots, API.setupProviders],
  office: [API.session, API.office, API.officeChat, API.officeLooks, API.officeLook, API.socket, ...APPROVALS],
  phone: [API.pushKey, API.pushSubscribe, API.notices, API.dailySummary, ...APPROVALS],
  // 관리 화면 넷(server 26차 4번): 이 차수의 틀은 세션만 얻는다(내용은 뒤 차수).
  account: [API.session],
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
  history: [API.session],
};

/** 승인 카드 읽기 경로 `/api/approvals/{id}`(폰과 같다, 번호는 주소 글자로 감싼다). */
export function approvalPath(id: number | string): string {
  return API.approvals + encodeURIComponent(String(id));
}

/** 승인 · 거절 경로 /api/approvals/{id}/approve · reject(폰과 같다). */
export function decidePath(id: number | string, verb: Verb): string {
  return approvalPath(id) + "/" + verb;
}
