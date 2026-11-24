// 관리 화면이 함께 쓰는 작은 도구(server 27 · 28차, 템플릿 · 규칙 화면): 같은 출처 경로 부르기 · 요소 만들기 ·
// 읽기 실패 글. 3D에 닿지 않는다.

/** 읽기 실패 글(E57). */
export const LOAD_FAILED = "불러오지 못함 · 다시 시도해 주세요. 서버와 연결되지 않았거나 잠시 응답하지 않습니다.";

/** 서버 답(상태 · JSON 몸통). */
export interface Reply {
  status: number;
  ok: boolean;
  body: unknown;
}

/** 같은 출처 경로를 부른다. 몸통을 JSON으로 읽고, 읽을 수 없으면 `null`. */
export async function call(method: "GET" | "POST", url: string, body?: unknown): Promise<Reply> {
  const init: RequestInit = { method, credentials: "same-origin", cache: "no-store" };
  if (body !== undefined) {
    init.headers = { "Content-Type": "application/json" };
    init.body = JSON.stringify(body);
  }
  const reply = await fetch(url, init);
  let parsed: unknown = null;
  try {
    parsed = await reply.json();
  } catch {
    parsed = null;
  }
  return { status: reply.status, ok: reply.ok, body: parsed };
}

/** 태그 · 클래스 · 글로 요소 하나. 글은 글자(`textContent`)로 넣는다. */
export function el<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
