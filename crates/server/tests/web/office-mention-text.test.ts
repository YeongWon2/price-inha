// 팀 방 멘션 글(server 33차 1번 · 5번, C18 · E43 · E44 · N11 · V58): 보낼 글 `@[이름]`과 가림, `@` 찾기 ·
// 거르기 · 목록 순환, 서버 안내 → 글, 멘션 글 몸통. 원본 DeskRPG 동작(metaverse-office-refs.md D절).
import { describe, expect, it } from "vitest";
import {
  MENTION_EMPTY,
  NO_MATCH_HINT,
  NO_MENTION_HINT,
  cycle,
  filterMembers,
  hasMention,
  hintText,
  mentionQuery,
  mentionTag,
  serializePieces,
  teamMessageBody,
  type Member,
  type Piece,
} from "../../web/src/office/mention-text";

const ROSTER: Member[] = [
  { agent: "news", name: "뉴스 공시 시세" },
  { agent: "strategy", name: "전략" },
  { agent: "risk", name: "리스크" },
  { agent: "review", name: "복기" },
  { agent: "trader", name: "매매" },
  { agent: "leader", name: "팀장" },
];

describe("보낼 글 모양", () => {
  it("칩은 보낼 때 @[이름]이 되고 이름 속 \\ [ ]는 \\로 가린다", () => {
    // 검증: V58 C18
    expect(mentionTag("리스크")).toBe("@[리스크]");
    expect(mentionTag("뉴스 공시 시세")).toBe("@[뉴스 공시 시세]");
    expect(mentionTag("a[b]c")).toBe("@[a\\[b\\]c]");
    expect(mentionTag("x\\y")).toBe("@[x\\\\y]");
  });

  it("조각을 차례로 잇고 글은 다듬지 않는다", () => {
    // 검증: V58 C18
    const pieces: Piece[] = [
      { kind: "text", text: "안녕 " },
      { kind: "mention", agent: "risk", name: "리스크" },
      { kind: "text", text: " " },
      { kind: "mention", agent: "strategy", name: "전략" },
      { kind: "text", text: " 어때요?" },
    ];
    expect(serializePieces(pieces)).toBe("안녕 @[리스크] @[전략] 어때요?");
    expect(hasMention(pieces)).toBe(true);
    expect(hasMention([{ kind: "text", text: "@리스크 괄호 없는 글" }])).toBe(false);
    expect(serializePieces([])).toBe("");
  });

  it("멘션 글 몸통에는 담당이 없다(서버가 멘션으로 정한다)", () => {
    // 검증: V58 C18
    const body = teamMessageBody("@[리스크] 안녕", "m-1");
    expect(body).toEqual({ room: "team", text: "@[리스크] 안녕", sourceMessageId: "m-1" });
    expect("agent" in body).toBe(false);
  });
});

describe("@ 찾기", () => {
  it("줄 처음이나 빈칸 뒤의 @에서만 열린다", () => {
    // 검증: V58 N11
    expect(mentionQuery("@", 1)).toEqual({ start: 0, query: "" });
    expect(mentionQuery("@리", 2)).toEqual({ start: 0, query: "리" });
    expect(mentionQuery("안녕 @리스", 6)).toEqual({ start: 3, query: "리스" });
    expect(mentionQuery("첫 줄\n@전", 6)).toEqual({ start: 4, query: "전" });
    expect(mentionQuery("mail@b", 6)).toBeNull();
    expect(mentionQuery("안녕", 2)).toBeNull();
  });

  it("찾는 말에 빈칸이 들어오면 닫히고, 커서 앞만 본다", () => {
    // 검증: V58 N11
    expect(mentionQuery("@리스 크", 5)).toBeNull();
    expect(mentionQuery("@리스 크", 3)).toEqual({ start: 0, query: "리스" });
    expect(mentionQuery("@리 안녕", 1)).toEqual({ start: 0, query: "" });
  });

  it("이름의 부분 포함으로 거르고(대소문자 무시) 빈 말이면 모두다", () => {
    // 검증: V58 N11
    expect(filterMembers(ROSTER, "").map((m) => m.agent)).toEqual(ROSTER.map((m) => m.agent));
    expect(filterMembers(ROSTER, "리").map((m) => m.agent)).toEqual(["risk"]);
    expect(filterMembers(ROSTER, "공시").map((m) => m.agent)).toEqual(["news"]);
    expect(filterMembers(ROSTER, "없음")).toEqual([]);
    const latin: Member[] = [{ agent: "a", name: "Alpha" }, { agent: "b", name: "beta" }];
    expect(filterMembers(latin, "AL").map((m) => m.agent)).toEqual(["a"]);
    expect(filterMembers(latin, "Ta").map((m) => m.agent)).toEqual(["b"]);
  });

  it("목록 이동은 끝과 처음을 돈다", () => {
    // 검증: V58
    expect(cycle(0, 3, 1)).toBe(1);
    expect(cycle(2, 3, 1)).toBe(0);
    expect(cycle(0, 3, -1)).toBe(2);
    expect(cycle(0, 0, 1)).toBe(0);
  });
});

describe("서버 안내", () => {
  it("멘션 없음 · 맞지 않음 안내 글", () => {
    // 검증: V58 E43 E44
    expect(NO_MENTION_HINT).toBe("@로 담당을 부르거나 공지로 보내세요");
    expect(NO_MATCH_HINT).toBe("지목한 상대가 이 방에 없어 아무도 답하지 않았습니다.");
    expect(hintText("no_mention")).toBe(NO_MENTION_HINT);
    expect(hintText("no_match")).toBe(NO_MATCH_HINT);
    expect(hintText(undefined)).toBeNull();
    expect(hintText("other")).toBeNull();
    expect(MENTION_EMPTY.length).toBeGreaterThan(0);
  });
});
