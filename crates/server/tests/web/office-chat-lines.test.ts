// 채팅 패널 줄(server 23차 8번, 원본 DeskRPG 화면 — docs/research/deskrpg.md 11절): 내 글은 오른쪽, 담당 글 · 논의
// 줄은 왼쪽에 굵은 이름 + 글, 답을 기다리는 담당은 `○○: 생각 중` 줄(답이 오면 사라진다).
import { describe, expect, it } from "vitest";
import { PENDING_TEXT, chatLine, pendingLabel } from "../../web/src/office/chat-lines";

const NAMES: Record<string, string> = { risk: "리스크 담당", leader: "팀장" };

describe("채팅 줄의 쪽", () => {
  it("내 글은 오른쪽, 이름 없이", () => {
    // 검증: V14
    expect(chatLine({ kind: "me", key: "m1", text: "삼성전자 어때?" }, NAMES)).toEqual({
      key: "m1",
      side: "right",
      name: null,
      text: "삼성전자 어때?",
      tag: null,
      pending: false,
    });
  });

  it("담당 답은 왼쪽에 굵은 이름 + 글", () => {
    // 검증: V14
    const line = chatLine({ kind: "reply", key: "r1", agent: "risk", status: "complete", content: "손절선 95,000원" }, NAMES);
    expect(line).toEqual({
      key: "r1",
      side: "left",
      name: "리스크 담당",
      text: "손절선 95,000원",
      tag: null,
      pending: false,
    });
  });

  it("논의 줄도 왼쪽에 굵은 이름 + 글, 종목을 작게 붙인다", () => {
    // 검증: V14
    const line = chatLine({ kind: "talk", key: "talk-3", agent: "leader", text: "오늘은 지켜봅니다", topic: "day" }, NAMES);
    expect(line).toEqual({
      key: "talk-3",
      side: "left",
      name: "팀장",
      text: "오늘은 지켜봅니다",
      tag: "day",
      pending: false,
    });
  });

  it("이름을 모르면 id를 쓴다", () => {
    // 검증: V14
    expect(chatLine({ kind: "talk", key: "talk-4", agent: "scout", text: "공시", topic: "005930" }, NAMES).name).toBe(
      "scout",
    );
  });
});

describe("생각 중 줄", () => {
  it("답을 기다리는 담당(대기 · 생각 중 · 글 없는 쓰는 중)은 `○○: 생각 중` 줄", () => {
    // 검증: V14
    expect(PENDING_TEXT).toBe("생각 중");
    for (const status of ["queued", "thinking", "writing"]) {
      const line = chatLine({ kind: "reply", key: "r1", agent: "risk", status }, NAMES);
      expect(line).toMatchObject({ side: "left", name: "리스크 담당", text: "생각 중", pending: true });
      expect(pendingLabel(line)).toBe("리스크 담당: 생각 중");
    }
  });

  it("답이 오면(글이 생기면) 생각 중 줄이 사라지고 글 줄이 된다", () => {
    // 검증: V14 C5
    const waiting = chatLine({ kind: "reply", key: "r1", agent: "risk", status: "thinking" }, NAMES);
    expect(waiting.pending).toBe(true);
    const streaming = chatLine({ kind: "reply", key: "r1", agent: "risk", status: "writing", content: "손절선은" }, NAMES);
    expect(streaming).toMatchObject({ key: "r1", side: "left", text: "손절선은", pending: false });
    const done = chatLine({ kind: "reply", key: "r1", agent: "risk", status: "complete", content: "손절선은 95,000원" }, NAMES);
    expect(done).toMatchObject({ key: "r1", text: "손절선은 95,000원", pending: false });
  });

  it("실패한 답은 생각 중 줄이 아니다", () => {
    // 검증: V14
    const failed = chatLine({ kind: "reply", key: "r2", agent: "leader", status: "failed" }, NAMES);
    expect(failed.pending).toBe(false);
    expect(failed.side).toBe("left");
  });
});
