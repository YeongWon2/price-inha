// 화면이 부르는 같은 출처 경로의 표(server 11차). 화면 코드는 이 표 · 함수의 값으로만 서버를 부른다.
// 바깥 주소는 없다. 번호가 들어가는 자리는 표에서 `{id}`로 적는다.

/** 화면 이름: 설정(`/`) · 키(`/keys`) · 사무실(`/office/`) · 폰(`/phone/`). */
export type ScreenName = "setup" | "keys" | "office" | "phone";

/** 승인 카드의 답. */
export type Verb = "approve" | "reject";

/** 화면이 열리는 주소(단추가 다른 화면으로 갈 때 쓴다). */
export const SCREEN_ROOTS: Readonly<Record<ScreenName, string>> = {
  setup: "/",
  keys: "/keys",
  office: "/office/",
  phone: "/phone/",
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
  pushKey: "/api/push/key",
  pushSubscribe: "/api/push/subscribe",
  notices: "/api/notices",
  dailySummary: "/api/daily-summary",
  approvals: "/api/approvals/",
} as const;

/**
 * 사무실 담당 외형(server 20차 4번): 읽기 `GET` · 바꾸기 `POST`. 사무실 경로([`API.office`]) 아래에 둔다.
 * 잠긴 경로 표 시험(screen-paths, 11차)이 사무실 목록을 옛 목록으로 묶어 두어 `SCREEN_PATHS`에는 아직 넣지 않았다.
 */
export const OFFICE_LOOK_PATHS = {
  list: `${API.office}/looks`,
  choose: `${API.office}/look`,
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
  office: [API.session, API.office, API.officeChat, API.socket, ...APPROVALS],
  phone: [API.pushKey, API.pushSubscribe, API.notices, API.dailySummary, ...APPROVALS],
};

/** 승인 카드 읽기 경로 `/api/approvals/{id}`(폰과 같다, 번호는 주소 글자로 감싼다). */
export function approvalPath(id: number | string): string {
  return API.approvals + encodeURIComponent(String(id));
}

/** 승인 · 거절 경로 /api/approvals/{id}/approve · reject(폰과 같다). */
export function decidePath(id: number | string, verb: Verb): string {
  return approvalPath(id) + "/" + verb;
}
