// 채팅 패널 줄의 쪽과 모양(server 23차 8번, 원본 DeskRPG 화면 — docs/research/deskrpg.md 11절): 내 글은 오른쪽 녹색
// 말풍선, 담당 글 · 논의 줄은 왼쪽에 굵은 이름 + 글, 답을 기다리는 담당은 `○○: 생각 중` 줄(답이 오면 사라진다).

/** 채팅 패널에 그릴 것 하나. */
export type ChatEntry =
  | { kind: "me"; key: string; text: string }
  | { kind: "reply"; key: string; agent: string; status: string; content?: string }
  | { kind: "talk"; key: string; agent: string; text: string; topic: string };

/** 채팅 패널의 줄 하나. */
export interface ChatLine {
  key: string;
  /** 내 글은 오른쪽, 담당 글 · 논의 줄 · 생각 중 줄은 왼쪽. */
  side: "right" | "left";
  /** 굵게 보일 담당 이름(내 글은 `null`). */
  name: string | null;
  text: string;
  /** 논의 줄의 작은 표시(종목 코드 또는 "day"). 다른 줄은 `null`. */
  tag: string | null;
  /** 답을 기다리는 `○○: 생각 중` 줄인가. */
  pending: boolean;
}

/** 생각 중 줄의 글. */
export const PENDING_TEXT = "생각 중";

/**
 * 채팅 줄 하나. 담당 답은 상태가 `complete`이면 그 글, 글이 온 쓰는 중이면 지금까지의 글, 글이 아직 없으면
 * (`queued` · `thinking` · 빈 `writing`) [`PENDING_TEXT`] 줄(`pending`)이다. `failed`는 생각 중 줄이 아니다. 이름은 `names`에서 찾고 없으면 id.
 */
export function chatLine(entry: ChatEntry, names: Readonly<Record<string, string>>): ChatLine {
  void entry;
  void names;
  throw new Error("todo");
}

/** 생각 중 줄의 한 줄 글자: `○○: 생각 중`. */
export function pendingLabel(line: ChatLine): string {
  void line;
  throw new Error("todo");
}
