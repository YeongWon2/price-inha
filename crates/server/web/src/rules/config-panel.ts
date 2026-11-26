// 규칙 화면의 금지 종목 · 업종표 묶음 하나의 상태와 서버 부르기(server 35차 3번): 읽기(`GET`) · 초안 고치기 · 저장
// (`POST`). 값 검사는 입력 편의일 뿐이고 최종 판단은 서버다(서버 답으로 다시 그린다). 그리기는 config-view.ts.
import "./config.css";
import { API } from "../paths";
import { call, el } from "../ui/call";
import {
  addBanned,
  addSector,
  draftOf,
  readConfig,
  removeBanned,
  removeSector,
  saveBody,
  saveMessage,
  type AddResult,
} from "./config-model";
import { renderConfig, renderConfigFailed, type ConfigHandlers, type ConfigState } from "./config-view";

/** 더하기 입력 칸 이름(다시 그릴 때 적던 글을 지키거나 비운다). */
const BANNED_INPUTS = ["banned-market", "banned-symbol"];
const SECTOR_INPUTS = ["sector-symbol", "sector-name"];

/** 금지 종목 · 업종표 묶음. [`ConfigPanel.element`]를 화면에 붙이고, 고치면 그 안만 다시 그린다. */
export class ConfigPanel {
  private readonly holder = el("div", "rul-cfg-holder");
  private state: ConfigState | null = null;
  private readonly handlers: ConfigHandlers = {
    addBanned: (market, symbol) => this.add(addBanned(this.draft(), market, symbol), BANNED_INPUTS),
    removeBanned: (index) => this.edit(() => removeBanned(this.draft(), index)),
    addSector: (symbol, sector) => this.add(addSector(this.draft(), symbol, sector), SECTOR_INPUTS),
    removeSector: (index) => this.edit(() => removeSector(this.draft(), index)),
    save: () => void this.save(),
  };

  /** 화면에 붙일 자리(늘 같은 요소). */
  element(): HTMLElement {
    return this.holder;
  }

  /** 지금 값을 읽어 초안을 새로 만든다. 못 읽으면 이 묶음만 못 읽음. */
  async load(): Promise<void> {
    let reply = null;
    try {
      const answer = await call("GET", API.rulesConfig);
      reply = answer.ok ? readConfig(answer.body) : null;
    } catch {
      reply = null;
    }
    this.state = reply ? { reply, draft: draftOf(reply), problem: null, saved: null, saving: false } : null;
    this.render();
  }

  private draft() {
    if (!this.state) throw new Error("읽기 전에는 고치지 않는다");
    return this.state.draft;
  }

  private edit(next: () => ConfigState["draft"]): void {
    if (!this.state || this.state.saving) return;
    this.state = { ...this.state, draft: next(), problem: null, saved: null };
    this.render();
  }

  /** 줄 더하기: 되면 그 입력 칸을 비우고, 안 되면 까닭을 보이고 적던 글을 남긴다. */
  private add(result: AddResult, inputs: readonly string[]): void {
    if (!this.state || this.state.saving) return;
    const typed = this.typed();
    if (result.ok) {
      this.state = { ...this.state, draft: result.draft, problem: null, saved: null };
      for (const name of inputs) if (!name.endsWith("market")) typed.delete(name);
    } else {
      this.state = { ...this.state, problem: result.problem, saved: null };
    }
    this.render(typed);
  }

  /** 초안을 통째로 보낸다. 200이면 서버가 답한 값으로 다시 그리고, 실패면 오류 줄 — 초안은 그대로다. */
  private async save(): Promise<void> {
    if (!this.state || this.state.saving) return;
    const body = saveBody(this.state.draft);
    this.state = { ...this.state, saving: true, problem: null, saved: null };
    this.render(this.typed());
    let status = 0;
    let answer: unknown = null;
    try {
      const reply = await call("POST", API.rulesConfig, body);
      status = reply.status;
      answer = reply.body;
    } catch {
      status = 0;
    }
    const message = saveMessage(status, answer);
    const read = status === 200 ? readConfig(answer) : null;
    const next = read ? { reply: read, draft: draftOf(read) } : {};
    this.state = { ...this.state, ...next, saving: false, saved: message };
    this.render(this.typed());
  }

  /** 지금 적혀 있는 더하기 입력 칸의 글. */
  private typed(): Map<string, string> {
    const typed = new Map<string, string>();
    for (const node of this.holder.querySelectorAll<HTMLInputElement | HTMLSelectElement>("input[name], select[name]")) {
      typed.set(node.name, node.value);
    }
    return typed;
  }

  private render(typed: Map<string, string> = new Map()): void {
    const card = this.state ? renderConfig(this.state, this.handlers) : renderConfigFailed();
    for (const [name, value] of typed) {
      const node = card.querySelector<HTMLInputElement | HTMLSelectElement>(`[name="${name}"]`);
      if (node) node.value = value;
    }
    this.holder.replaceChildren(card);
  }
}
