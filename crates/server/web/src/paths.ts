// 화면이 부르는 같은 출처 경로의 표(server 11차). 화면 코드는 이 표 · 함수의 값으로만 서버를 부른다.
// 바깥 주소는 없다. 번호가 들어가는 자리는 표에서 `{id}`로 적는다.
// N1 뼈대: 값과 몸통은 N2가 채운다.

/** 화면 이름: 설정(`/`) · 키(`/keys`) · 사무실(`/office/`) · 폰(`/phone/`). */
export type ScreenName = "setup" | "keys" | "office" | "phone";

/** 승인 카드의 답. */
export type Verb = "approve" | "reject";

/** 각 화면이 부르는 경로(지금 화면과 같은 목록, 사무실은 승인 카드 경로가 더해진다). */
export const SCREEN_PATHS: Readonly<Record<ScreenName, readonly string[]>> = {
  setup: [],
  keys: [],
  office: [],
  phone: [],
};

/** 승인 카드 읽기 경로 `/api/approvals/{id}`(폰과 같다, 번호는 주소 글자로 감싼다). */
export function approvalPath(id: number | string): string {
  throw new Error(`아직 없음(N2): approvalPath(${id})`);
}

/** 승인 · 거절 경로 `/api/approvals/{id}/approve|reject`(폰과 같다). */
export function decidePath(id: number | string, verb: Verb): string {
  throw new Error(`아직 없음(N2): decidePath(${id}, ${verb})`);
}
