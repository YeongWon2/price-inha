// 템플릿 화면(server 27차 3번 · F6 · C28 · E5 · E28 · E57 · E58): 고르기 · 값 맞추기 · 상태 · 승격 기준 · 판 이력 ·
// 실전 운용 시작 · 멈추기. 서버는 `paths.ts`의 경로로만 부른다. 3D 라이브러리에 닿지 않는다(N21).
//
// 화면 계약(시험이 고르는 표시, 칸 · 표 · 이력은 view.ts):
// - 스타일 고르기 `[data-role="styles"] button[data-style=단타|스윙|중장기의 글자]` → `POST /api/templates {style}` 뒤
//   목록을 다시 읽는다.
// - 지금 템플릿 상태 배지 `[data-role="state"]`, 값 저장 `button[data-action="save"]` → `POST /api/templates/adjust`
//   `{id, changes}`, 결과 문구 `[data-role="save-result"]`(상한 넘음은 칸 옆 안내도).
// - "실전 운용 시작" `button[data-action="promote"]`: 승격대기일 때만 누를 수 있다. 누르면 승인 카드
//   `GET /api/approvals/{번호}`를 읽고 확인(`confirm`) 뒤에만 `POST /api/approvals/{번호}/approve {fingerprint}`.
//   그 옆에 실전 안내 `[data-role="live-notice"]`가 상태와 상관없이 늘 보인다.
// - "멈추기" `button[data-action="stop"]`: 실전중일 때만 있다. 확인(`confirm`) 뒤에만 `POST /api/templates/{id}/stop`,
//   그 뒤 목록을 다시 읽는다.
// - 읽기에 실패하면 본문에 "불러오지 못함 · 다시 시도" 오류 줄과 `button[data-action="retry"]`(E57). 메뉴는 그대로다.

import "../ui/admin.css";
import "./templates.css";
import { API, approvalPath, decidePath, templateStopPath } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { badge, button, card, emptyState, errorLine, loading, type BadgeTone } from "../ui/parts";
import {
  LIVE_NOTICE,
  STYLES,
  canPromote,
  canStop,
  currentTemplate,
  fieldRows,
  promotionRows,
  revisionRows,
  saveMessage,
  stateLabel,
  styleLabel,
  type FieldRow,
  type SaveMessage,
  type StyleName,
  type TemplateEntry,
  type TemplateState,
  type TemplatesReply,
} from "./model";
import { changedValues, liveNotice, renderFields, renderPromotion, renderRevisions, showFieldNotes } from "./view";

/** 상태 배지 색. */
const STATE_TONES: Readonly<Record<TemplateState, BadgeTone>> = {
  simulating: "neutral",
  promotion_pending: "warning",
  live: "success",
  stopped: "danger",
};

/** 결과 문구 줄. `tone`은 글의 색(토큰)만 바꾼다. */
function resultLine(role: string, message: SaveMessage | null): HTMLElement {
  const node = el("p", "tpl-result");
  node.dataset.role = role;
  node.setAttribute("aria-live", "polite");
  if (message) {
    node.textContent = message.text;
    node.dataset.tone = message.kind;
  }
  return node;
}

/** 화면 하나의 상태와 그리기. */
class TemplatesScreen {
  private readonly body = el("div", "tpl-body");
  private view: TemplatesReply | null = null;
  private entry: TemplateEntry | null = null;
  private rows: FieldRow[] = [];
  private saveNote: SaveMessage | null = null;
  private actionNote: SaveMessage | null = null;
  private busy = false;

  constructor(main: HTMLElement) {
    main.append(this.body);
  }

  /** 목록을 읽어 다시 그린다. 실패하면 오류 줄과 다시 시도. */
  async load(): Promise<void> {
    try {
      const reply = await call("GET", API.templatesView);
      if (!reply.ok || typeof reply.body !== "object" || reply.body === null) throw new Error(`view ${reply.status}`);
      this.view = reply.body as TemplatesReply;
      this.entry = currentTemplate(this.view);
      this.rows = this.entry ? fieldRows(this.entry) : [];
      this.render();
    } catch {
      this.view = null;
      this.entry = null;
      this.rows = [];
      this.renderFailed();
    }
  }

  /** 첫 그림 전: 고르기 · 불러오는 중 · 실전 운용. */
  renderLoading(): void {
    this.body.replaceChildren(this.stylesCard(), loading(), this.liveCard());
  }

  private renderFailed(): void {
    const retry = button("다시 시도", "accent");
    retry.dataset.action = "retry";
    retry.addEventListener("click", () => {
      retry.disabled = true;
      void this.load();
    });
    const failed = el("div", "tpl-failed");
    failed.append(errorLine(LOAD_FAILED), retry);
    this.body.replaceChildren(failed, this.stylesCard(), this.liveCard());
  }

  private render(): void {
    const parts: HTMLElement[] = [this.stylesCard()];
    if (this.entry) parts.push(this.valuesCard(this.entry), this.promotionCard(this.entry));
    parts.push(this.liveCard());
    if (this.entry) parts.push(this.revisionsCard(this.entry));
    this.body.replaceChildren(...parts);
  }

  /** (가) 스타일 고르기와 지금 쓰는 템플릿. */
  private stylesCard(): HTMLElement {
    const choices = el("div", "tpl-styles");
    choices.dataset.role = "styles";
    choices.setAttribute("role", "group");
    choices.setAttribute("aria-label", "스타일 고르기");
    for (const [style, label] of STYLES) {
      const pick = button(label, this.entry?.style === style && this.entry.state !== "stopped" ? "accent" : "default");
      pick.dataset.style = style;
      pick.addEventListener("click", () => void this.choose(style, label));
      choices.append(pick);
    }
    const hint = el("p", "tpl-hint", "고르면 그 스타일의 기본값으로 새 템플릿을 만들고 모의 운용부터 시작합니다.");
    return card("스타일 고르기", hint, choices, this.currentLine());
  }

  /** 지금 쓰는 템플릿 줄(번호 · 스타일 · 판 · 상태 배지). */
  private currentLine(): HTMLElement {
    if (!this.entry) {
      return this.view ? emptyState("아직 고른 템플릿이 없습니다. 위에서 스타일을 고르세요.") : el("span", "");
    }
    const line = el("div", "tpl-current");
    line.append(el("span", "tpl-current-label", "지금 쓰는 템플릿"));
    line.append(el("strong", "tpl-current-name", `${styleLabel(this.entry.style)} · ${this.entry.id}번 · 판 ${this.entry.revision}`));
    const state = badge(stateLabel(this.entry.state), STATE_TONES[this.entry.state]);
    state.dataset.role = "state";
    line.append(state);
    return line;
  }

  /** (나) 값 맞추기. */
  private valuesCard(entry: TemplateEntry): HTMLElement {
    const parts: HTMLElement[] = [];
    if (entry.state === "live") {
      const notice = el("p", "tpl-notice", "실전중 템플릿입니다 — 값을 바꾸면 승인이 필요합니다. 위험을 키우는 변경은 승인 뒤 다음 거래일부터 적용됩니다.");
      notice.dataset.role = "approval-notice";
      notice.dataset.tone = "warning";
      parts.push(notice);
    } else if (entry.state === "promotion_pending" || entry.state === "stopped") {
      const why = entry.state === "stopped" ? "멈춘 템플릿은 값이 잠겨 있습니다. 바꾸려면 스타일을 새로 고르세요." : "승격대기 중에는 기준을 통과한 값이 잠겨 있습니다.";
      const notice = el("p", "tpl-notice", why);
      notice.dataset.role = "locked-notice";
      parts.push(notice);
    }
    const fields = renderFields(this.rows);
    const wrap = el("div", "tpl-table-wrap");
    wrap.append(fields);
    parts.push(wrap);
    const save = button("저장", "accent");
    save.dataset.action = "save";
    save.disabled = this.rows.every((row) => row.locked);
    const result = resultLine("save-result", this.saveNote);
    save.addEventListener("click", () => void this.save(entry, fields, result, save));
    fields.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && event.target instanceof HTMLInputElement) save.click();
    });
    const actions = el("div", "tpl-actions");
    actions.append(save, result);
    parts.push(actions);
    return card("값 맞추기", ...parts);
  }

  /** (다) 승격 기준 표. */
  private promotionCard(entry: TemplateEntry): HTMLElement {
    const rows = promotionRows(entry);
    const short = rows.filter((row) => row.short).length;
    const lead = el("p", "tpl-hint", short === 0 ? "모든 기준을 채웠습니다." : `모자란 기준 ${short}개 — 모두 채우면 실전 승격 승인을 요청합니다.`);
    const wrap = el("div", "tpl-table-wrap");
    wrap.append(renderPromotion(rows));
    return card("승격 기준", lead, wrap);
  }

  /** (마 · 바) 실전 운용 시작 · 멈추기와 늘 보이는 실전 안내. */
  private liveCard(): HTMLElement {
    const promote = button("실전 운용 시작", "accent");
    promote.dataset.action = "promote";
    promote.disabled = !canPromote(this.entry);
    promote.addEventListener("click", () => void this.promote(promote));
    const actions = el("div", "tpl-actions");
    actions.append(promote);
    if (canStop(this.entry)) {
      const stop = button("멈추기", "danger");
      stop.dataset.action = "stop";
      stop.addEventListener("click", () => void this.stop(stop));
      actions.append(stop);
    }
    const hint = el("p", "tpl-hint", this.promoteHint());
    return card("실전 운용", hint, actions, liveNotice(), resultLine("action-result", this.actionNote));
  }

  private promoteHint(): string {
    const entry = this.entry;
    if (!entry) return "템플릿을 고르고 승격 기준을 모두 채우면 실전 운용을 시작할 수 있습니다.";
    switch (entry.state) {
      case "promotion_pending":
        return entry.approval ? `승격 기준을 모두 채웠습니다. 승인 번호 ${entry.approval}로 실전 운용을 시작할 수 있습니다.` : "승격 승인 요청을 기다리는 중입니다.";
      case "live":
        return "실전 운용 중입니다. 멈추면 다시 실전으로 돌아갈 수 없고 스타일을 새로 골라야 합니다.";
      case "stopped":
        return "멈춘 템플릿입니다. 다시 운용하려면 스타일을 새로 고르세요.";
      default:
        return "모의 운용 중입니다. 승격 기준을 모두 채우면 실전 운용을 시작할 수 있습니다.";
    }
  }

  /** (라) 판 이력. */
  private revisionsCard(entry: TemplateEntry): HTMLElement {
    return card("판 이력", renderRevisions(revisionRows(entry)));
  }

  private async choose(style: StyleName, label: string): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      const reply = await call("POST", API.templates, { style });
      this.saveNote = null;
      this.actionNote = reply.ok ? null : { kind: "failed", text: `${label} 템플릿을 만들지 못했습니다. 잠시 뒤 다시 시도해 주세요.` };
    } catch {
      this.actionNote = { kind: "failed", text: `${label} 템플릿을 만들지 못했습니다. 서버와 연결을 확인해 주세요.` };
    } finally {
      this.busy = false;
    }
    await this.load();
  }

  private async save(entry: TemplateEntry, fields: HTMLElement, result: HTMLElement, save: HTMLButtonElement): Promise<void> {
    if (this.busy) return;
    const changes = changedValues(fields, this.rows);
    if (Object.keys(changes).length === 0) {
      showFieldNotes(fields, []);
      this.show(result, { kind: "failed", text: "바꾼 값이 없습니다. 새 값 칸에 수를 넣은 뒤 저장하세요." });
      return;
    }
    this.busy = true;
    save.disabled = true;
    let message: SaveMessage;
    try {
      const reply = await call("POST", API.templatesAdjust, { id: entry.id, changes });
      message = saveMessage({ status: reply.status, body: reply.body }, this.rows);
    } catch {
      message = { kind: "failed", text: "저장하지 못했습니다. 서버와 연결을 확인해 주세요." };
    }
    this.busy = false;
    save.disabled = false;
    showFieldNotes(fields, message.kind === "over_cap" ? message.fields : []);
    this.show(result, message);
    if (message.kind === "saved") {
      this.saveNote = message;
      await this.load();
    }
  }

  private show(node: HTMLElement, message: SaveMessage): void {
    node.textContent = message.text;
    node.dataset.tone = message.kind;
  }

  private async promote(promote: HTMLButtonElement): Promise<void> {
    const number = this.entry?.approval;
    if (this.busy || !number) return;
    this.busy = true;
    promote.disabled = true;
    try {
      const read = await call("GET", approvalPath(number));
      const shown = (read.body ?? {}) as { summary?: unknown; fingerprint?: unknown };
      if (!read.ok || typeof shown.fingerprint !== "string") {
        this.actionNote = { kind: "failed", text: "승인 카드를 읽지 못했습니다. 이미 처리됐거나 기한이 지났을 수 있습니다." };
      } else {
        const summary = typeof shown.summary === "string" ? shown.summary : "실전 승격";
        if (!confirm(`실전 운용을 시작할까요?\n\n${summary}\n승인 번호 ${number}\n\n${LIVE_NOTICE}`)) {
          this.busy = false;
          promote.disabled = false;
          return;
        }
        const answer = await call("POST", decidePath(number, "approve"), { fingerprint: shown.fingerprint });
        const body = (answer.body ?? {}) as { result?: unknown; reason?: unknown };
        this.actionNote =
          answer.ok && body.result === "approved"
            ? { kind: "saved", text: "승인했습니다. 실전 운용을 시작합니다(실제 증권사 주문은 나가지 않습니다)." }
            : { kind: "failed", text: `승인되지 않았습니다${typeof body.reason === "string" ? `(${body.reason})` : ""}.` };
      }
    } catch {
      this.actionNote = { kind: "failed", text: "승인하지 못했습니다. 서버와 연결을 확인해 주세요." };
    }
    this.busy = false;
    await this.load();
  }

  private async stop(stop: HTMLButtonElement): Promise<void> {
    const entry = this.entry;
    if (this.busy || !entry) return;
    if (!confirm(`${entry.id}번 템플릿의 실전 운용을 멈출까요?\n멈춘 뒤에는 다시 실전으로 돌아갈 수 없고, 스타일을 새로 골라야 합니다.`)) return;
    this.busy = true;
    stop.disabled = true;
    try {
      const reply = await call("POST", templateStopPath(entry.id));
      this.actionNote = reply.ok
        ? { kind: "saved", text: "실전 운용을 멈췄습니다." }
        : { kind: "failed", text: "멈추지 못했습니다. 이미 실전중이 아닐 수 있습니다." };
    } catch {
      this.actionNote = { kind: "failed", text: "멈추지 못했습니다. 서버와 연결을 확인해 주세요." };
    }
    this.busy = false;
    await this.load();
  }
}

/**
 * 본문 자리(`main`)에 템플릿 화면을 그린다: 세션을 얻고(`POST /api/session`) 목록을 읽어 그린다.
 * 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
 */
export async function mountTemplates(main: HTMLElement | null): Promise<void> {
  if (!main) return;
  const screen = new TemplatesScreen(main);
  screen.renderLoading();
  try {
    await call("POST", API.session);
  } catch {
    // 세션을 얻지 못해도 읽기를 해 본다: 실패하면 오류 줄과 다시 시도가 보인다.
  }
  await screen.load();
}
