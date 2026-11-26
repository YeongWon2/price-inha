// 키 화면(`/keys`)의 세션(기능 H 2번 · backlog "하네스 36차"): 그 컴퓨터에서 세션 없이 바로 열었으면 설정 화면처럼
// 세션을 얻는다(`POST /api/session`, 본문 없이 — 그 컴퓨터는 키 없이 세션을 받는다).
// - 먼저 등록 여부 읽기(`GET /api/setup/slots`)를 해 본다: 되면 세션이 이미 있다(다른 기기가 접속 키로 들어와 연
//   경우 포함) — 세션을 청하지 않는다.
// - 401이고 주소 이름이 그 컴퓨터(루프백)일 때만 세션을 한 번 청한다. 다른 기기 주소에서 접속 키 없이 청하면
//   서버가 틀린 시도로 세므로 청하지 않는다(그때는 접속 키로 다시 들어와야 한다).
// 서버가 판단한다: 화면의 주소 이름 확인은 쓸데없는 청하기를 줄일 뿐, 관문은 서버의 상대 주소 확인이다.

import { API } from "../paths";

/** 서버를 부르는 함수(시험은 가짜를 넣는다). */
export type Fetcher = (input: string, init?: RequestInit) => Promise<Response>;

/** 세션이 없을 때의 HTTP 상태. */
const NO_SESSION = 401;

/** 주소 이름이 그 컴퓨터(루프백)인가: `localhost` · `127.x.x.x` · `::1`(대괄호 포함). */
export function isLoopbackHost(host: string): boolean {
  const name = host.toLowerCase();
  if (name === "localhost" || name === "::1" || name === "[::1]") return true;
  const parts = name.split(".");
  return parts.length === 4 && parts[0] === "127" && parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) <= 255);
}

/** 세션을 쓸 수 있게 됐는지 돌려준다(이미 있었거나 그 컴퓨터라 새로 얻었으면 참). 실패해도 던지지 않는다. */
export async function ensureKeysSession(fetcher: Fetcher, host: string): Promise<boolean> {
  try {
    const probe = await fetcher(API.setupSlots, { credentials: "same-origin", cache: "no-store" });
    if (probe.ok) return true;
    if (probe.status !== NO_SESSION || !isLoopbackHost(host)) return false;
    const reply = await fetcher(API.session, { method: "POST", credentials: "same-origin", cache: "no-store" });
    return reply.ok;
  } catch {
    return false;
  }
}
