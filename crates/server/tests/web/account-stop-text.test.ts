// 계좌 화면 팀 카드의 전체 정지 안내 글(server 41차 7번 · 40차 검토 · R14-1 · V85): 까닭에 맞는 글 **하나만** 보인다 —
// 최대 손실(또는 까닭 모름)이면 `FULL_STOP_TEXT`만, 내가 정지(`stop_cause` `"owner_stop"`)면 `OWNER_STOP_TEXT`만.
import { afterEach, describe, expect, it } from "vitest";
import { FULL_STOP_TEXT, OWNER_STOP_TEXT } from "../../web/src/account/model";
import type { TeamReply } from "../../web/src/account/model";
import { renderTeam } from "../../web/src/account/view";

function stopped(stop_cause: string | null | undefined): TeamReply {
  return { state: "full_stop", plan: null, reason: null, stop_cause };
}

/** 팀 카드를 그려 붙인다. */
function draw(team: TeamReply): HTMLElement {
  const node = renderTeam(team, () => {});
  document.body.append(node);
  return node;
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("계좌 화면 · 전체 정지 안내 글", () => {
  it("내가 정지면 '내가 전체 정지를 눌렀습니다' 글 하나만 보이고 최대 손실 글은 없다", () => {
    // 검증: V85 R14-1 F4
    const card = draw(stopped("owner_stop"));
    const text = card.textContent ?? "";
    expect(OWNER_STOP_TEXT.startsWith("내가 전체 정지를 눌렀습니다")).toBe(true);
    expect(text).toContain(OWNER_STOP_TEXT);
    expect(text).not.toContain(FULL_STOP_TEXT);
    expect(card.querySelectorAll(".acc-stop-text").length, "안내 글은 하나").toBe(1);
    expect(card.querySelectorAll('button[data-action="restart"]').length).toBe(1);
  });

  it("최대 손실이면 지금 글 하나만 보이고 내가 정지 글은 없다", () => {
    // 검증: V85 R3-3 R14-1
    const card = draw(stopped("max_loss"));
    const text = card.textContent ?? "";
    expect(text).toContain(FULL_STOP_TEXT);
    expect(text).not.toContain(OWNER_STOP_TEXT);
    expect(card.querySelectorAll(".acc-stop-text").length, "안내 글은 하나").toBe(1);
  });

  it("까닭을 모르면(없음) 지금 글 하나만 보인다", () => {
    // 검증: V85 R14-1
    for (const cause of [null, undefined]) {
      document.body.innerHTML = "";
      const card = draw(stopped(cause));
      const text = card.textContent ?? "";
      expect(text).toContain(FULL_STOP_TEXT);
      expect(text).not.toContain(OWNER_STOP_TEXT);
      expect(card.querySelectorAll(".acc-stop-text").length).toBe(1);
    }
  });
});
