// 팀 방 멘션 글(server 33차 1번 · 5번, C18 · E43 · E44 · N11 · V58): 글 조각 ↔ 보낼 글 `@[이름]`, `@` 목록 거르기,
// 서버 안내(`hint`) → 보이는 글. 원본 DeskRPG 동작 그대로(docs/research/metaverse-office-refs.md D절,
// docs/research/deskrpg.md 10절):
// - 보낼 때만 칩을 `@[이름]`으로 바꾼다. 이름 속 `\` `[` `]` 앞에 `\`를 붙인다(서버 `pi_office::mention_tag`와 같다).
// - `@` 목록은 `@`가 줄 처음이거나 빈칸 바로 뒤일 때만 뜨고, `@` 뒤 글(찾는 말)에 빈칸이 들어오면 닫힌다(`a@b` 제외).
// - 거르기는 소문자로 바꾼 이름의 부분 포함(`includes`). 목록 이동은 ↑↓ 순환.
// DOM 없이 글과 조각만 다룬다(칩 에디터 DOM은 `mention-editor.ts`).

/** 명단 한 줄: 담당 id와 화면에 보이는 이름(멘션은 이 이름과 정확히 같아야 한다). */
export interface Member {
  agent: string;
  name: string;
}

/** 글 칸의 조각: 글이거나 멘션 칩. */
export type Piece = { kind: "text"; text: string } | { kind: "mention"; agent: string; name: string };

/** 멘션 없는 팀 방 글에 보이는 안내(E44 · 4b2). */
export const NO_MENTION_HINT = "@로 담당을 부르거나 공지로 보내세요";

/** 멘션한 이름이 명단에 없을 때 보낸 사람에게만 보이는 안내(E43 · 4d1, 원본 글 그대로). */
export const NO_MATCH_HINT = "지목한 상대가 이 방에 없어 아무도 답하지 않았습니다.";

/** `@` 목록에 맞는 담당이 없을 때의 줄. */
export const MENTION_EMPTY = "해당하는 담당이 없습니다";

/** 이름 → 보낼 글의 멘션 `@[이름]`(이름 속 `\` `[` `]` 앞에 `\`). */
export function mentionTag(name: string): string {
  let tag = "@[";
  for (const c of name) {
    if (c === "\\" || c === "[" || c === "]") tag += "\\";
    tag += c;
  }
  return `${tag}]`;
}

/** 조각들 → 서버로 보낼 글. 글 조각은 그대로, 칩은 [`mentionTag`]로 잇는다(다듬지 않는다). */
export function serializePieces(pieces: readonly Piece[]): string {
  return pieces.map((p) => (p.kind === "text" ? p.text : mentionTag(p.name))).join("");
}

/** 조각에 멘션 칩이 하나라도 있는가. */
export function hasMention(pieces: readonly Piece[]): boolean {
  return pieces.some((p) => p.kind === "mention");
}

/** 빈칸(줄바꿈 포함)인가. */
function isSpace(c: string): boolean {
  return /\s/.test(c);
}

/**
 * 글 `text`의 커서 `caret`(글자 위치) 앞에서 열린 `@` 찾기. `@`가 줄 처음이거나 빈칸(줄바꿈 포함) 바로 뒤이고,
 * `@`부터 커서까지에 빈칸이 없을 때만 `{ start: "@"의 위치, query: "@" 뒤부터 커서까지 }`. 아니면 `null`.
 */
export function mentionQuery(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, Math.max(0, Math.min(caret, text.length)));
  for (let i = before.length - 1; i >= 0; i -= 1) {
    const c = before[i];
    if (isSpace(c)) return null;
    if (c === "@") {
      if (i > 0 && !isSpace(before[i - 1])) return null;
      return { start: i, query: before.slice(i + 1) };
    }
  }
  return null;
}

/** 찾는 말로 명단을 거른다: 소문자로 바꾼 이름에 소문자로 바꾼 찾는 말이 들어 있는 담당(명단 순서). 빈 말은 모두. */
export function filterMembers(members: readonly Member[], query: string): Member[] {
  const want = query.toLowerCase();
  return members.filter((m) => m.name.toLowerCase().includes(want));
}

/** 목록 이동: `count`개 가운데 `index`에서 `delta`(±1)만큼, 끝에서 처음으로 · 처음에서 끝으로 돈다. `count`가 0이면 0. */
export function cycle(index: number, count: number, delta: 1 | -1): number {
  if (count <= 0) return 0;
  return (((index + delta) % count) + count) % count;
}

/** 서버 답의 `hint` → 보낸 사람에게 보일 안내. `"no_mention"` → [`NO_MENTION_HINT`], `"no_match"` → [`NO_MATCH_HINT`], 그 밖은 `null`. */
export function hintText(hint: unknown): string | null {
  if (hint === "no_mention") return NO_MENTION_HINT;
  if (hint === "no_match") return NO_MATCH_HINT;
  return null;
}

/** 팀 방 멘션 글의 몸통: `{"room": "team", "text", "sourceMessageId"}`(`agent` 없음 — 서버가 멘션으로 답할 담당을 정한다). */
export function teamMessageBody(text: string, sourceMessageId: string): { room: "team"; text: string; sourceMessageId: string } {
  return { room: "team", text, sourceMessageId };
}
