// 사무실 승인 카드(server 10c 14번 · 11차 4번). `office.approval` 이벤트
// `{number(승인 번호 글자 — 서버 ApprovalNumber), summary(가린 요약), short_code(지문 앞 6자), status}`로 카드를 그리고,
// 승인 · 거절은 폰과 같은 경로(/api/approvals/{id}/approve · reject) · 같은 지문 대조로 보낸다.
//
// 카드 모양(시험이 보는 약속):
// - 카드: `root` 아래 `[data-approval="<번호>"]` 하나.
// - 요약: `[data-part="summary"]`(글로만 넣는다 — textContent), 지문 앞 6자: `[data-part="code"]`,
//   결과 글: `[data-part="result"]`, 단추: `button[data-verb="approve"]` · `button[data-verb="reject"]`.

import { approvalPath, decidePath, type Verb } from "../paths";

/** 사무실 승인 이벤트 종류. */
export const KIND_OFFICE_APPROVAL = "office.approval";

/** 이벤트 몸통. */
export interface OfficeApproval {
  /** 승인 번호(서버 `ApprovalNumber`, hex 글자). */
  number: string;
  summary: string;
  short_code: string;
  status: string;
}

/** `fetch`와 같은 모양(시험에서 바꿔 끼운다). */
export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

/** 승인 · 거절의 결과: 보냄 · 지문이 카드와 다름(보내지 않음) · 실패. */
export type DecideOutcome = "sent" | "mismatch" | "failed";

const PENDING = "pending";
const CODE_LENGTH = 6;

function parse(payload: unknown): OfficeApproval | null {
  if (typeof payload !== "object" || payload === null) return null;
  const p = payload as Record<string, unknown>;
  if (typeof p.number !== "string" || p.number === "") return null;
  if (typeof p.summary !== "string" || typeof p.short_code !== "string" || typeof p.status !== "string") {
    return null;
  }
  return { number: p.number, summary: p.summary, short_code: p.short_code, status: p.status };
}

function findCard(root: HTMLElement, number: string): HTMLElement | null {
  for (const card of root.querySelectorAll<HTMLElement>("[data-approval]")) {
    if (card.dataset.approval === number) return card;
  }
  return null;
}

function part(card: HTMLElement, name: string): HTMLElement | null {
  return card.querySelector<HTMLElement>(`[data-part="${name}"]`);
}

function textPart(tag: string, name: string, className = ""): HTMLElement {
  const element = document.createElement(tag);
  element.dataset.part = name;
  if (className) element.className = className;
  return element;
}

function verbButton(root: HTMLElement, number: string, verb: Verb, fetcher: FetchLike): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.dataset.verb = verb;
  button.textContent = verb === "approve" ? "승인" : "거절";
  button.addEventListener("click", () => {
    void decideApproval(root, number, verb, fetcher);
  });
  return button;
}

function newCard(root: HTMLElement, number: string, fetcher: FetchLike): HTMLElement {
  const card = document.createElement("article");
  card.className = "approval-card";
  card.dataset.approval = number;
  const title = document.createElement("h2");
  title.textContent = "승인이 필요해요";
  const code = document.createElement("p");
  code.className = "muted";
  code.append("내용 지문 앞 6자 ", textPart("strong", "code"));
  const row = document.createElement("div");
  row.className = "row";
  row.append(verbButton(root, number, "approve", fetcher), verbButton(root, number, "reject", fetcher));
  const result = textPart("p", "result", "muted");
  result.setAttribute("role", "status");
  card.append(title, textPart("p", "summary"), code, row, result);
  root.append(card);
  return card;
}

/**
 * 이벤트 한 건을 카드에 반영한다. `pending`이면 그 번호의 카드를 그리거나 고치고(같은 번호는 한 장),
 * 아니면 그 카드를 닫는다(지운다). 모양이 맞지 않는 몸통은 무시한다.
 */
export function applyApproval(root: HTMLElement, payload: unknown, fetcher: FetchLike): void {
  const approval = parse(payload);
  if (!approval) return;
  const existing = findCard(root, approval.number);
  if (approval.status !== PENDING) {
    existing?.remove();
    return;
  }
  const card = existing ?? newCard(root, approval.number, fetcher);
  const summary = part(card, "summary");
  const code = part(card, "code");
  if (summary) summary.textContent = approval.summary;
  if (code) code.textContent = approval.short_code.slice(0, CODE_LENGTH);
}

function say(card: HTMLElement | null, text: string): void {
  const result = card ? part(card, "result") : null;
  if (result) result.textContent = text;
}

function lockVerbs(card: HTMLElement | null, locked: boolean): void {
  if (!card) return;
  for (const button of card.querySelectorAll<HTMLButtonElement>("button[data-verb]")) button.disabled = locked;
}

/**
 * 승인 · 거절을 보낸다. 보내는 동안 그 카드의 두 단추를 막고(같은 카드에서 두 번 눌러도 한 번만 나간다),
 * 실패하면 다시 켜며, 보냈으면 막힌 채 둔다(이미 결정됨).
 *
 * 폰과 같이 GET /api/approvals/{id}로 전체 지문을 받아, 그 앞 6자가 카드에 보인
 * 지문 앞 6자와 같을 때만 POST /api/approvals/{id}/{verb} `{"fingerprint": 전체 지문}`을 보낸다.
 */
export async function decideApproval(
  root: HTMLElement,
  number: string,
  verb: Verb,
  fetcher: FetchLike,
): Promise<DecideOutcome> {
  const card = findCard(root, number);
  if (card?.dataset.deciding === "1") return "failed";
  if (card) card.dataset.deciding = "1";
  lockVerbs(card, true);
  const outcome = await send(card, number, verb, fetcher);
  if (card) delete card.dataset.deciding;
  if (outcome !== "sent") lockVerbs(card, false);
  return outcome;
}

async function send(
  card: HTMLElement | null,
  number: string,
  verb: Verb,
  fetcher: FetchLike,
): Promise<DecideOutcome> {
  const shown = card ? (part(card, "code")?.textContent ?? "") : "";
  try {
    const reply = await fetcher(approvalPath(number), { credentials: "same-origin", cache: "no-store" });
    if (!reply.ok) {
      say(card, "승인 내용을 받지 못했어요: " + reply.status);
      return "failed";
    }
    const body = (await reply.json()) as { fingerprint?: unknown };
    const fingerprint = typeof body.fingerprint === "string" ? body.fingerprint : "";
    if (shown.length !== CODE_LENGTH || fingerprint.slice(0, CODE_LENGTH) !== shown) {
      say(card, "지문이 카드와 달라요. 보내지 않았어요.");
      return "mismatch";
    }
    const sent = await fetcher(decidePath(number, verb), {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ fingerprint }),
    });
    if (!sent.ok) {
      say(card, "처리하지 못했어요: " + sent.status);
      return "failed";
    }
    say(card, verb === "approve" ? "승인했어요." : "거절했어요.");
    return "sent";
  } catch (error) {
    say(card, "처리하지 못했어요: " + (error instanceof Error ? error.message : String(error)));
    return "failed";
  }
}
