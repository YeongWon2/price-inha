// 응답 목록 정리(server 12차 3번, 옛 office `web_prune`): 화면이 밀려난 줄을 버릴 때 끝난 응답(complete · failed)은
// 응답 표에서도 지우고, 오는 중인 응답은 남긴다. 정리 함수는 `src/office/trim.ts`로 뺀 것을 돌려 본다.
import { describe, expect, it } from "vitest";
import { KEEP_LINES, trimLines, type Line, type ResponseStatus } from "../../web/src/office/trim";

const mains = import.meta.glob<string>("../../web/src/office/main.ts", { query: "?raw", import: "default", eager: true });

const me = (id: string): Line => ({ kind: "me", id, room: "team", agent: "leader", text: id });
const resp = (requestId: string): Line => ({ kind: "resp", requestId });

/** 앞에 `head` 줄을 두고 뒤를 내 말로 채워 모두 `total`줄을 만든다. */
function fill(head: Line[], total: number): Line[] {
  const lines = [...head];
  for (let i = lines.length; i < total; i++) lines.push(me(`m${i}`));
  return lines;
}

describe("응답 목록 정리", () => {
  it("한 대화에 남기는 줄 수는 사무실 화면과 같은 200줄이다", () => {
    // 검증: C5
    expect(KEEP_LINES).toBe(200);
  });

  it("줄이 넘치지 않으면 아무것도 버리지 않는다", () => {
    // 검증: C5
    const lines = fill([resp("r1")], KEEP_LINES);
    const responses = new Map<string, ResponseStatus>([["r1", { status: "complete" }]]);
    trimLines(lines, responses);
    expect(lines.length).toBe(KEEP_LINES);
    expect(responses.has("r1")).toBe(true);
  });

  it("넘친 오래된 줄을 버리고 최근 줄만 남긴다", () => {
    // 검증: C5
    const lines = fill([], KEEP_LINES + 5);
    trimLines(lines, new Map());
    expect(lines.length).toBe(KEEP_LINES);
    expect(lines[0]).toEqual(me("m5"));
    expect(lines[lines.length - 1]).toEqual(me(`m${KEEP_LINES + 4}`));
  });

  it("밀려난 끝난 응답(complete · failed)은 응답 표에서도 지우고, 오는 중인 응답은 남긴다", () => {
    // 검증: C5
    const head = [resp("done"), resp("fail"), resp("coming"), resp("queued"), me("old")];
    const lines = fill([...head, resp("kept")], KEEP_LINES + head.length);
    const responses = new Map<string, ResponseStatus>([
      ["done", { status: "complete" }],
      ["fail", { status: "failed" }],
      ["coming", { status: "streaming" }],
      ["queued", { status: "queued" }],
      ["kept", { status: "complete" }],
    ]);
    trimLines(lines, responses);
    expect(lines.length).toBe(KEEP_LINES);
    expect(lines[0]).toEqual(resp("kept"));
    expect(responses.has("done")).toBe(false);
    expect(responses.has("fail")).toBe(false);
    expect(responses.get("coming")).toEqual({ status: "streaming" });
    expect(responses.get("queued")).toEqual({ status: "queued" });
    // 남은 줄의 응답은 끝났어도 지우지 않는다.
    expect(responses.get("kept")).toEqual({ status: "complete" });
  });

  it("남길 줄 수를 받으면 그만큼만 남긴다", () => {
    // 검증: C5
    const lines = [resp("a"), resp("b"), me("c")];
    const responses = new Map<string, ResponseStatus>([
      ["a", { status: "failed" }],
      ["b", { status: "complete" }],
    ]);
    trimLines(lines, responses, 1);
    expect(lines).toEqual([me("c")]);
    expect(responses.size).toBe(0);
  });

  it("사무실 화면은 이 정리 함수를 쓴다(같은 동작을 따로 두지 않는다)", () => {
    // 검증: C5
    const main = Object.values(mains)[0] ?? "";
    expect(main.length, "src/office/main.ts가 있다").toBeGreaterThan(0);
    expect(/import\s*\{[^}]*\btrimLines\b[^}]*\}\s*from\s*["']\.\/trim["']/.test(main), "main.ts가 ./trim의 trimLines를 불러온다").toBe(true);
    expect(main.includes("function trim("), "main.ts에 따로 둔 정리 함수가 없다").toBe(false);
  });
});
