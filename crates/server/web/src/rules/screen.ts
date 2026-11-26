// 규칙 화면(server 28차 2번 · F5 · C29 · V82 · E28 · E35 · E57): 지금 규칙 표 · 절대 상한 · 개선안 목록과 승인 · 거절.
// 서버는 `paths.ts`의 경로로만 부른다. 3D 라이브러리에 닿지 않는다(N21).
//
// 화면 계약(시험이 고르는 표시, 표 · 목록은 view.ts):
// - 띄우면 세션을 얻고(`POST /api/session`) 규칙(`GET /api/rules`)과 개선안 목록(`GET /api/proposals`)을 읽어 그린다.
//   템플릿이 없으면 규칙 표 자리에 빈 상태 `[data-role="no-template"]`, 개선안이 없으면 `[data-role="no-proposals"]`.
// - 제안됨 줄의 승인 · 거절 단추 → `POST /api/proposals/answer {id, number, fingerprint, answer: "approve" | "reject"}`
//   (번호 · 지문은 목록의 것을 그대로). 답한 뒤 규칙 · 목록을 다시 읽어 그리고, 결과 문구는 그 개선안 줄의
//   `[data-role="answer-result"]`에 남는다. 답하는 동안 그 줄의 단추는 막힌다.
// - 읽기에 실패하면 본문에 "불러오지 못함 · 다시 시도" 오류 줄(`role="alert"`)과 `button[data-action="retry"]`(E57).
//   메뉴는 그대로다.
// - (server 35차 3번) 금지 종목 · 업종표 묶음(`GET` · `POST /api/rules/config`, 그리기는 config-view.ts): 띄울 때 한 번
//   읽고, 이 묶음만 못 읽으면 그 묶음 안에만 못 읽음을 보인다(다른 묶음은 그대로). 고치던 줄(초안)은 저장하거나
//   화면을 다시 열 때까지 남는다 — 개선안에 답한 뒤 다시 읽을 때도 지우지 않는다.
// - (기능 L · 결정 30) "팀 지침(AI가 배운 것)" 묶음(`GET /api/guidelines` · 끄기, 그리기는 guide-panel.ts): 띄울 때 한 번
//   따로 불러와 읽고, 개선안 목록 뒤에 둔다. 이 묶음만 못 읽으면 그 묶음 안에만 못 읽음을 보인다.
import "../ui/admin.css";
import "./rules.css";
import { API } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { button, card, emptyState, errorLine, loading } from "../ui/parts";
import {
  answerMessage,
  capRows,
  proposalRows,
  ruleRows,
  type AnswerMessage,
  type ProposalRow,
  type ProposalsReply,
  type RulesReply,
} from "./model";
import { renderCaps, renderProposals, renderRules } from "./view";
import type { ConfigPanel } from "./config-panel";
import type { GuidePanel } from "./guide-panel";

/** 빈 상태 줄에 `data-role`을 단다. */
function empty(role: string, text: string): HTMLElement {
  const node = emptyState(text);
  node.dataset.role = role;
  return node;
}

/** 안내 글 줄. */
function hint(text: string): HTMLElement {
  return el("p", "rul-hint", text);
}

/** 화면 하나의 상태와 그리기. */
class RulesScreen {
  private readonly body = el("div", "rul-body");
  private rows: ProposalRow[] = [];
  /** 개선안 번호 → 그 줄에 남길 답 결과 문구. */
  private readonly notes = new Map<number, AnswerMessage>();
  private busy = false;
  /** 금지 종목 · 업종표 묶음(읽기 · 초안 · 저장). 첫 화면을 가볍게(N21) 따로 불러온다. */
  private config: ConfigPanel | null = null;
  /** 그 묶음을 불러오기 전 · 못 불러왔을 때의 자리. */
  private readonly configSlot = el("div", "rul-cfg-holder");
  /** (기능 L) 팀 지침 묶음. 첫 화면을 가볍게 따로 불러온다. */
  private guide: GuidePanel | null = null;
  /** 그 묶음을 불러오기 전 · 못 불러왔을 때의 자리. */
  private readonly guideSlot = el("div", "rul-guide-holder");

  constructor(main: HTMLElement) {
    main.append(this.body);
  }

  renderLoading(): void {
    this.body.replaceChildren(loading());
  }

  /**
   * 규칙 · 개선안 목록을 읽어 다시 그린다. 하나라도 실패하면 오류 줄과 다시 시도. `withConfig`면 금지 종목 ·
   * 업종표도 다시 읽는다(그 묶음의 실패는 그 묶음 안에만 보인다).
   */
  async load(withConfig = true): Promise<void> {
    const configRead = withConfig ? this.loadConfig() : Promise.resolve();
    const guideRead = withConfig ? this.loadGuides() : Promise.resolve();
    try {
      const [rules, proposals] = await Promise.all([call("GET", API.rules), call("GET", API.proposals)]);
      if (!rules.ok || typeof rules.body !== "object" || rules.body === null) throw new Error(`rules ${rules.status}`);
      if (!proposals.ok || typeof proposals.body !== "object" || proposals.body === null) {
        throw new Error(`proposals ${proposals.status}`);
      }
      this.render(rules.body as RulesReply, proposals.body as ProposalsReply);
    } catch {
      this.rows = [];
      this.renderFailed();
    }
    await Promise.all([configRead, guideRead]);
  }

  /** (기능 L) 팀 지침 묶음을 (처음이면 불러와) 읽고, 그려 둔 자리를 그 묶음으로 바꾼다. */
  private async loadGuides(): Promise<void> {
    if (!this.guide) {
      this.guideSlot.replaceChildren(loading());
      try {
        const { GuidePanel } = await import("./guide-panel");
        this.guide ??= new GuidePanel();
      } catch {
        this.guideSlot.replaceChildren(el("p", "rul-hint", "팀 지침을 불러오지 못했습니다. 화면을 다시 열어 주세요."));
        return;
      }
    }
    const panel = this.guide;
    await panel.load();
    if (this.guideSlot.isConnected) this.guideSlot.replaceWith(panel.element());
  }

  /** 금지 종목 · 업종표 묶음을 (처음이면 불러와) 읽고, 그려 둔 자리를 그 묶음으로 바꾼다. */
  private async loadConfig(): Promise<void> {
    if (!this.config) {
      this.configSlot.replaceChildren(loading());
      try {
        const { ConfigPanel } = await import("./config-panel");
        this.config ??= new ConfigPanel();
      } catch {
        const retry = button("다시 시도", "accent");
        retry.dataset.action = "retry";
        retry.addEventListener("click", () => void this.loadConfig());
        this.configSlot.replaceChildren(errorLine(LOAD_FAILED), retry);
        return;
      }
    }
    const panel = this.config;
    await panel.load();
    if (this.configSlot.isConnected) this.configSlot.replaceWith(panel.element());
  }

  private render(rules: RulesReply, proposals: ProposalsReply): void {
    const ruleList = ruleRows(rules);
    const current =
      rules.template === null || ruleList.length === 0
        ? empty("no-template", "아직 고른 템플릿이 없어 적용 중인 규칙이 없습니다. 템플릿 화면에서 스타일을 고르세요.")
        : el("div", "rul-table-wrap");
    if (ruleList.length > 0) current.append(renderRules(ruleList));
    const rulesCard = card("지금 규칙", hint(this.rulesHint(rules)), current);

    const capsCard = card(
      "절대 상한",
      hint("원칙이 정한 한도입니다. 개선안도 템플릿 값도 이 상한을 넘을 수 없습니다."),
      renderCaps(capRows(rules)),
    );

    this.rows = (proposals.proposals ?? []).length > 0 ? proposalRows(proposals) : [];
    const list =
      this.rows.length === 0
        ? empty("no-proposals", "아직 개선안이 없습니다. 복기에서 같은 실수가 반복되면 개선안이 올라옵니다.")
        : renderProposals(this.rows, this.notes);
    list.addEventListener("click", (event) => this.onClick(event));
    const proposalsCard = card(
      "개선안",
      hint("위험을 키우는 개선안은 승인 뒤 냉각중이 되고 다음 거래일부터 반영됩니다."),
      list,
    );
    const guides = this.guide?.element() ?? this.guideSlot;
    this.body.replaceChildren(rulesCard, capsCard, this.config?.element() ?? this.configSlot, proposalsCard, guides);
  }

  private rulesHint(rules: RulesReply): string {
    if (rules.template === null) return "지금 쓰는 템플릿의 위험 규칙을 보여 줍니다.";
    return `지금 쓰는 템플릿 ${rules.template}번의 규칙입니다. 값은 템플릿 화면이나 승인한 개선안으로만 바뀝니다.`;
  }

  private renderFailed(): void {
    const retry = button("다시 시도", "accent");
    retry.dataset.action = "retry";
    retry.addEventListener("click", () => {
      this.renderLoading();
      void this.load();
    });
    const box = el("div", "rul-failed");
    box.append(errorLine(LOAD_FAILED), retry);
    this.body.replaceChildren(box);
  }

  private onClick(event: Event): void {
    const target = event.target instanceof Element ? event.target.closest<HTMLButtonElement>("button[data-action]") : null;
    const item = target?.closest<HTMLElement>("[data-proposal]");
    if (!target || !item) return;
    const action = target.dataset.action;
    if (action !== "approve" && action !== "reject") return;
    const row = this.rows.find((candidate) => String(candidate.id) === item.dataset.proposal);
    if (row) void this.answer(row, action, item);
  }

  /** 개선안 하나에 답한다(번호 · 지문은 목록의 것 그대로). 답하는 동안 그 줄의 단추를 막는다. */
  private async answer(row: ProposalRow, answer: "approve" | "reject", item: HTMLElement): Promise<void> {
    if (this.busy || !row.canAnswer) return;
    this.busy = true;
    for (const node of item.querySelectorAll<HTMLButtonElement>("button[data-action]")) node.disabled = true;
    try {
      const reply = await call("POST", API.proposalAnswer, {
        id: row.id,
        number: row.number,
        fingerprint: row.fingerprint,
        answer,
      });
      this.notes.set(row.id, answerMessage({ status: reply.status, body: reply.body }));
    } catch {
      this.notes.set(row.id, { kind: "failed", text: "서버와 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요." });
    }
    this.busy = false;
    await this.load(false);
  }
}

/**
 * 본문 자리(`main`)에 규칙 화면을 그린다: 세션을 얻고 규칙 · 개선안 목록을 읽어 그린다.
 * 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
 */
export async function mountRules(main: HTMLElement | null): Promise<void> {
  if (!main) return;
  const screen = new RulesScreen(main);
  screen.renderLoading();
  try {
    await call("POST", API.session);
  } catch {
    // 세션을 얻지 못해도 읽기를 해 본다: 실패하면 오류 줄과 다시 시도가 보인다.
  }
  await screen.load();
}
