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
  if (entry.kind === "me") {
    return { key: entry.key, side: "right", name: null, text: entry.text, tag: null, pending: false };
  }
  const name = Object.hasOwn(names, entry.agent) ? names[entry.agent] : entry.agent;
  if (entry.kind === "talk") {
    return { key: entry.key, side: "left", name, text: entry.text, tag: entry.topic, pending: false };
  }
  const content = entry.content ?? "";
  const waiting = entry.status !== "complete" && entry.status !== "failed" && !content.trim();
  return {
    key: entry.key,
    side: "left",
    name,
    text: waiting ? PENDING_TEXT : content,
    tag: null,
    pending: waiting,
  };
}

/** 생각 중 줄의 한 줄 글자: `○○: 생각 중`. */
export function pendingLabel(line: ChatLine): string {
  return `${line.name ?? ""}: ${PENDING_TEXT}`;
}

/** (server 41차 3번) 메신저에서 온 팀 방 글에 붙는 표시(이름 옆 작은 덧말). */
export const VIA_MESSENGER_TEXT = "메신저에서";

/** 메신저에서 온 팀 방 글 한 줄: 채팅 패널에 그릴 것과 이름 옆 덧말. */
export interface ViaMessenger {
  /** 내 글 줄(`kind: "me"`, `key`는 `sourceMessageId`, `text`는 글). */
  entry: ChatEntry;
  /** [`VIA_MESSENGER_TEXT`]. */
  note: string;
}

/**
 * (server 41차 3번) 실시간 연결로 받은 `office.message` 내용 하나를 읽는다. `via`가 `"messenger"`이고 `room` ·
 * `text` · `sourceMessageId`가 글자이면 그 글의 내 글 줄과 "메신저에서" 덧말을, 그 밖(화면에서 보낸 글 — `via` 없음 ·
 * 모양이 틀림)이면 `null`을 준다. 사무실 화면은 이 줄을 채팅 패널에 더하고 덧말을 이름 옆에 단다(`chatLineElement`의 `note`).
 */
export function fromMessenger(payload: unknown): ViaMessenger | null {
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return null;
  const { via, room, text, sourceMessageId } = payload as Record<string, unknown>;
  if (via !== "messenger") return null;
  if (typeof room !== "string" || typeof text !== "string" || typeof sourceMessageId !== "string") return null;
  return { entry: { kind: "me", key: sourceMessageId, text }, note: VIA_MESSENGER_TEXT };
}
