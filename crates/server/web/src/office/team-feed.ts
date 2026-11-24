// 채팅 "팀" 탭 줄 합치기(server 22차 2번, 스펙 R5-2 · F7-4): 사용자가 쓴 글 · 담당 답 · 팀의 논의 줄
// (`office.talk`)을 시각 순서로 한 줄씩 보인다. 논의 줄에는 어느 종목 논의인지 작게 붙인다.

/** 서버 이벤트 `office.talk`의 내용(`GET /api/office/talk`의 한 줄과 같다). `at`은 유닉스 밀리초. */
export interface Talk {
  room: string;
  agent: string;
  text: string;
  at: number;
  topic: string;
}

/** 팀 탭에 들어가는 것 하나. `key`가 같은 것은 한 번만 보인다(다시 이어 받을 때 겹친 줄). */
export type FeedEntry =
  | { kind: "me"; key: string; at: number; agent: string; text: string }
  | { kind: "reply"; key: string; at: number; agent: string; text: string }
  | { kind: "talk"; key: string; at: number; agent: string; text: string; topic: string };

/** 팀 탭에 그리는 줄. */
export interface FeedRow {
  kind: FeedEntry["kind"];
  key: string;
  at: number;
  /** 줄 앞의 이름: 담당 줄은 담당 이름, 내 글은 "나"로 시작한다. */
  who: string;
  text: string;
  /** 논의 줄의 작은 표시(종목 코드 또는 "day"). 다른 줄은 `null`. */
  tag: string | null;
}

/** `office.talk` 하나를 팀 탭 것으로. `key`는 이벤트 번호로 만든다(같은 번호는 같은 줄). */
export function talkEntry(talk: Talk, seq: number): FeedEntry {
  return { kind: "talk", key: `talk-${seq}`, at: talk.at, agent: talk.agent, text: talk.text, topic: talk.topic };
}

/**
 * 팀 탭 줄들: `key`가 겹치면 처음 것만 남기고, 시각(`at`) 순서로(같은 시각이면 넘긴 순서) 늘어놓는다.
 * 담당 이름은 `names`(담당 id → 이름)에서 찾고, 없으면 id를 그대로 쓴다.
 */
export function teamFeed(entries: readonly FeedEntry[], names: Readonly<Record<string, string>>): FeedRow[] {
  const seen = new Set<string>();
  const kept: { entry: FeedEntry; order: number }[] = [];
  entries.forEach((entry, order) => {
    if (seen.has(entry.key)) return;
    seen.add(entry.key);
    kept.push({ entry, order });
  });
  kept.sort((a, b) => a.entry.at - b.entry.at || a.order - b.order);
  return kept.map(({ entry }) => {
    const name = Object.hasOwn(names, entry.agent) ? names[entry.agent] : entry.agent;
    return {
      kind: entry.kind,
      key: entry.key,
      at: entry.at,
      who: entry.kind === "me" ? `나 → ${name}` : name,
      text: entry.text,
      tag: entry.kind === "talk" ? entry.topic : null,
    };
  });
}
