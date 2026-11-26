// 근거 카드 그리기(server 50차 · C23 · V68 · V69): 담당별 의견 줄(방향 · 확신도 막대 · 이유 한 줄 · 보는 기간 · 기권),
// 찬반 수, 팀장 결정 · 요약 · 뒤집힐 조건, 자료 이름. 옛 매매는 "기록 없음". 글은 글자로만 들어간다.
import { describe, expect, it } from "vitest";
import { readBasisCard } from "../../web/src/ui/basis-card";
import { renderBasisCard } from "../../web/src/ui/basis-card-view";
import { oldBasis, recordedBasis } from "./basis-card-fixture";

function pick<T extends Element>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

function draw(wire: unknown): HTMLElement {
  const node = renderBasisCard(readBasisCard(wire));
  document.body.replaceChildren(node);
  return node;
}

describe("근거 카드 그리기", () => {
  it("담당 줄마다 이름 · 방향 · 확신도 막대 · 이유 · 보는 기간이 보이고 차례는 받은 그대로다", () => {
    // 검증: V68 R6-4 C23
    const card = draw(recordedBasis());
    expect(card.dataset.role).toBe("basis-card");
    expect(card.textContent).toContain("근거 카드");
    const rows = [...card.querySelectorAll<HTMLElement>('[data-role="opinions"] > li')];
    expect(rows.map((row) => row.dataset.agent)).toEqual(["scout", "strategy", "risk", "review"]);
    const strategy = rows[1];
    expect(strategy.dataset.tone).toBe("bull");
    expect(pick(strategy, '[data-part="name"]').textContent).toBe("전략 담당");
    expect(pick(strategy, '[data-part="direction"]').textContent).toBe("강세");
    const meter = pick<HTMLElement>(strategy, '[data-part="confidence"]');
    expect(meter.getAttribute("role")).toBe("meter");
    expect(meter.getAttribute("aria-valuenow")).toBe("3");
    expect(meter.getAttribute("aria-valuemax")).toBe("3");
    expect(meter.getAttribute("aria-label")).toContain("높음");
    expect(meter.querySelectorAll('[data-on="true"]').length).toBe(3);
    expect(pick(strategy, '[data-part="reason"]').textContent).toBe("실적 개선이 이어진다");
    expect(pick(strategy, '[data-part="horizon"]').textContent).toBe("보는 기간 20거래일");
    const risk = rows[2];
    expect(pick(risk, '[data-part="confidence"]').querySelectorAll('[data-on="true"]').length).toBe(2);
    expect(pick(rows[3], '[data-part="reason"]').textContent).toBe("없음");
  });

  it("기권한 담당은 '기권'으로 보이고 0점 · 확신도 막대가 없다", () => {
    // 검증: V68 E53
    const card = draw(recordedBasis());
    const scout = pick<HTMLElement>(card, '[data-agent="scout"]');
    expect(scout.dataset.abstained).toBe("true");
    expect(scout.dataset.tone).toBe("abstain");
    expect(pick(scout, '[data-part="direction"]').textContent).toBe("기권");
    expect(scout.querySelector('[data-part="confidence"]')).toBeNull();
    expect(scout.textContent).not.toContain("0점");
    expect(pick<HTMLElement>(card, '[data-agent="strategy"]').dataset.abstained).toBeUndefined();
  });

  it("찬반 수 · 팀장 결정 · 요약 · 뒤집힐 조건 · 자료 이름이 보인다", () => {
    // 검증: V69 R6-5 R4-4
    const card = draw(recordedBasis());
    expect(pick(card, '[data-role="votes"]').textContent).toContain("강세 1 · 약세 1 · 중립 1 · 기권 1");
    const leader = pick<HTMLElement>(card, '[data-role="leader"]');
    expect(pick(leader, '[data-part="decision"]').textContent).toContain("사기");
    expect(pick(leader, '[data-part="summary"]').textContent).toBe("실적이 좋아지고 거래량이 늘었다. 비중은 작게 시작한다.");
    const flip = pick(leader, '[data-part="flip"]').textContent ?? "";
    expect(flip).toContain("이러면 뒤집힌다");
    expect(flip).toContain("거래량이 다시 줄면 뒤집힌다");
    expect(pick(card, '[data-role="sources"]').textContent).toContain("시세 · 주요사항보고서(자기주식취득결정)");
    expect(card.querySelector('[data-role="no-record"]')).toBeNull();
  });

  it("칸이 생기기 전 매매는 '기록 없음'과 자료 줄만 보인다", () => {
    // 검증: V69 E53
    const card = draw(oldBasis());
    expect(pick(card, '[data-role="no-record"]').textContent).toBe("기록 없음");
    expect(card.querySelector('[data-role="opinions"]')).toBeNull();
    expect(card.querySelector('[data-role="leader"]')).toBeNull();
    expect(pick(card, '[data-role="sources"]').textContent).toContain("시세");
  });

  it("이유 글의 꺾쇠는 글자로만 들어간다", () => {
    // 검증: V68
    const wire = recordedBasis();
    (wire.opinions as { reason: string }[])[1].reason = "<b>굵게</b>";
    const card = draw(wire);
    const reason = pick(card, '[data-agent="strategy"] [data-part="reason"]');
    expect(reason.textContent).toBe("<b>굵게</b>");
    expect(reason.querySelector("b")).toBeNull();
  });
});
