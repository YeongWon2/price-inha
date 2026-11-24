// 채팅 "팀" 탭에 팀의 대화(server 22차 2번, 스펙 R5-2 · F7-4): 사용자가 쓴 글 · 담당 답 · 논의 줄(`office.talk`)이
// 시각 순서로 보이고, 논의 줄은 "담당 이름 · 글"에 종목을 작게 붙인다. 다시 이어 받아 겹친 줄은 한 번만 보인다.
import { describe, expect, it } from "vitest";
import { talkEntry, teamFeed, type FeedEntry, type Talk } from "../../web/src/office/team-feed";

const NAMES: Record<string, string> = {
  scout: "뉴스 공시 시세 담당",
  strategy: "전략 담당",
  risk: "리스크 담당",
  leader: "팀장",
};

const talk = (agent: string, text: string, at: number, topic = "000660"): Talk => ({
  room: topic === "day" ? "day" : "trade-7",
  agent,
  text,
  at,
  topic,
});

describe("팀 탭 줄 합치기", () => {
  it("논의 줄을 이벤트 번호로 열쇠를 삼아 팀 탭 것으로 바꾼다", () => {
    // 검증: V14 C2
    const entry = talkEntry(talk("risk", "손절선 95,000원", 1_000), 42);
    expect(entry).toMatchObject({ kind: "talk", at: 1_000, agent: "risk", text: "손절선 95,000원", topic: "000660" });
    expect(talkEntry(talk("risk", "다른 글", 2_000), 42).key).toBe(entry.key);
    expect(talkEntry(talk("risk", "손절선 95,000원", 1_000), 43).key).not.toBe(entry.key);
  });

  it("내 글 · 담당 답 · 논의 줄을 시각 순서로 늘어놓는다", () => {
    // 검증: V14 C5
    const entries: FeedEntry[] = [
      { kind: "me", key: "m1", at: 3_000, agent: "risk", text: "지금 위험은?" },
      talkEntry(talk("scout", "공시가 나왔습니다", 1_000), 1),
      { kind: "reply", key: "r1", at: 4_000, agent: "risk", text: "손절선 아래로는 버티지 않습니다" },
      talkEntry(talk("leader", "지켜보기로 합니다", 2_000), 2),
    ];
    const rows = teamFeed(entries, NAMES);
    expect(rows.map((r) => r.at)).toEqual([1_000, 2_000, 3_000, 4_000]);
    expect(rows.map((r) => r.kind)).toEqual(["talk", "talk", "me", "reply"]);
  });

  it("논의 줄은 담당 이름 · 글이고 종목(또는 day)을 작게 붙인다", () => {
    // 검증: V14
    const rows = teamFeed(
      [talkEntry(talk("strategy", "추세가 꺾였다", 1_000), 1), talkEntry(talk("leader", "오늘은 매매한다", 2_000, "day"), 2)],
      NAMES,
    );
    expect(rows[0]).toMatchObject({ who: "전략 담당", text: "추세가 꺾였다", tag: "000660" });
    expect(rows[1]).toMatchObject({ who: "팀장", text: "오늘은 매매한다", tag: "day" });
  });

  it("내 글과 담당 답에는 종목 표시가 없고, 내 글은 나로 시작한다", () => {
    // 검증: V14 C5
    const rows = teamFeed(
      [
        { kind: "me", key: "m1", at: 1, agent: "risk", text: "질문" },
        { kind: "reply", key: "r1", at: 2, agent: "risk", text: "답" },
      ],
      NAMES,
    );
    expect(rows[0].tag).toBeNull();
    expect(rows[0].who.startsWith("나")).toBe(true);
    expect(rows[1]).toMatchObject({ who: "리스크 담당", text: "답", tag: null });
  });

  it("이름을 모르는 담당은 id를 그대로 쓴다", () => {
    // 검증: V14
    const rows = teamFeed([talkEntry(talk("review", "복기", 1), 9)], NAMES);
    expect(rows[0].who).toBe("review");
  });

  it("같은 시각이면 넘긴 순서를 지킨다", () => {
    // 검증: V14
    const rows = teamFeed(
      [talkEntry(talk("scout", "첫째", 5), 1), talkEntry(talk("strategy", "둘째", 5), 2), talkEntry(talk("risk", "셋째", 5), 3)],
      NAMES,
    );
    expect(rows.map((r) => r.text)).toEqual(["첫째", "둘째", "셋째"]);
  });

  it("다시 이어 받아 겹친 줄은 한 번만 보인다", () => {
    // 검증: E3 C2
    const before = [talkEntry(talk("scout", "공시", 1), 10), talkEntry(talk("risk", "위험", 2), 11)];
    const replayed = [talkEntry(talk("risk", "위험", 2), 11), talkEntry(talk("leader", "판단", 3), 12)];
    const rows = teamFeed([...before, ...replayed], NAMES);
    expect(rows.map((r) => r.text)).toEqual(["공시", "위험", "판단"]);
  });
});
