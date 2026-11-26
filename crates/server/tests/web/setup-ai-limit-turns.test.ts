// 설정 화면의 하루 AI 호출 상한 안내(server 54차, 기능 F · N14 · C22): 상한은 논의 하나가 아니라 AI 호출(담당 턴)마다
// 하나로 센다. 안내 글이 그 뜻을 말하고(논의 하나가 대략 몇 번을 쓰는지), 예전 뜻("논의 한 번을 한 번")은 남지 않는다.
// 값 칸 · 지금 값 표시(#ai-limit-now)는 그대로다(저장된 값은 바꾸지 않는다 — 뜻만 설계서대로).
import { describe, expect, it } from "vitest";

const pages = import.meta.glob<string>("../../web/index.html", { query: "?raw", import: "default", eager: true });
const html = Object.values(pages)[0] ?? "";

function limitHint(): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  return doc.querySelector("#ai-limit-box .hint")?.textContent ?? "";
}

describe("하루 AI 호출 상한 안내 글", () => {
  it("AI 호출(담당 턴)마다 하나로 센다고 말하고 논의 하나가 대략 몇 번을 쓰는지 알려 준다", () => {
    // 검증: N14 C22 E52
    const hint = limitHint();
    expect(hint).toContain("하루 AI 호출 상한");
    expect(hint).toMatch(/AI를 부를 때마다/);
    expect(hint).toMatch(/논의 하나가 대략 \d+~\d+번/);
    expect(hint).not.toContain("논의 한 번을 한 번으로");
  });

  it("지금 값 표시와 바꾸는 칸은 그대로 있다", () => {
    // 검증: E30 N5
    const doc = new DOMParser().parseFromString(html, "text/html");
    expect(doc.querySelector("#ai-limit-now")).not.toBeNull();
    expect(doc.querySelector("#f-ai-limit")).not.toBeNull();
  });
});
