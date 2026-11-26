// 승인 카드 보강(server 50차 · C24 · R4-4 · V55): 주문 승인 카드(폰 · 사무실)에 담당 투표 수와 한도 적용 전 → 후를
// 보인다. 두 칸은 서버가 요청의 summary에서 그대로 꺼내 준 것(보여 주기용)이고, 승인 지문 · 잠금 규칙과는 상관없다.
// 글은 모두 글자(textContent)로 넣는다.
//
// 모양(시험이 고르는 표시): 자리 `[data-part="extras"]` 안에 투표 줄 `[data-part="votes"]`
// ("담당 투표 강세 n · 약세 n · 중립 n · 기권 n")과 한도 줄 `[data-part="clamp"]`("한도 적용 12주 → 9주(종목 비중 한도)").
// 두 칸이 다 없으면 자리를 숨긴다.
import "./approval-extras.css";

/** 한도 적용 한 줄(서버 글자 그대로). */
export interface ClampWire {
  limit: string;
  before: string;
  after: string;
  reason: string;
}

/** 승인 카드 보강 글. */
export interface ExtrasLines {
  /** 찬반 수 글, 투표 칸이 없으면 `null`. */
  votes: string | null;
  /** 한도 줄 글들(적용한 순서). */
  clamps: string[];
}

type Wire = Record<string, unknown>;

function isObject(value: unknown): value is Wire {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const VOTE_KEYS = ["bull", "bear", "neutral", "abstain"] as const;

function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
}

function isClamp(value: unknown): value is ClampWire {
  return (
    isObject(value) &&
    typeof value.limit === "string" &&
    typeof value.before === "string" &&
    typeof value.after === "string" &&
    typeof value.reason === "string"
  );
}

/** 승인 카드 몸통(모양이 틀린 칸은 없는 것으로 본다)에서 보강 글을 만든다. */
export function extrasLines(wire: unknown): ExtrasLines {
  const body = isObject(wire) ? wire : {};
  const votes = isObject(body.votes) ? body.votes : null;
  const counted = votes !== null && VOTE_KEYS.every((key) => isCount(votes[key]));
  const list: unknown[] = Array.isArray(body.clamps) ? body.clamps : [];
  return {
    votes: counted
      ? `강세 ${String(votes.bull)} · 약세 ${String(votes.bear)} · 중립 ${String(votes.neutral)} · 기권 ${String(votes.abstain)}`
      : null,
    clamps: list.filter(isClamp).map((c) => `${c.before}주 → ${c.after}주(${c.reason})`),
  };
}

function line(name: string, label: string, text: string): HTMLElement {
  const node = document.createElement("p");
  node.className = "approval-extra";
  node.dataset.part = name;
  const head = document.createElement("span");
  head.className = "approval-extra-label";
  head.textContent = label;
  node.append(head, " ", text);
  return node;
}

/** 자리에 보강 글을 그린다(앞의 것은 지운다). 보일 것이 없으면 자리를 숨긴다. */
export function renderApprovalExtras(box: HTMLElement | null, wire: unknown): void {
  if (!box) return;
  const lines = extrasLines(wire);
  const nodes: HTMLElement[] = [];
  if (lines.votes !== null) nodes.push(line("votes", "담당 투표", lines.votes));
  for (const clamp of lines.clamps) nodes.push(line("clamp", "한도 적용", clamp));
  box.replaceChildren(...nodes);
  box.hidden = nodes.length === 0;
}
