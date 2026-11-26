// 설정 화면의 담당 모델 고르기(server 57차 · R18-4 · C32 · V78)를 jsdom에 띄운다(가짜 fetch): 담당마다 고르기 ·
// 직접 입력 · 저장(POST 모양 · 답대로 다시 그림) · 기본값으로 되돌리기 · 틀린 값(400)의 오류 줄 · 다른 기기면 보기만 ·
// 비용 안내 줄 · 고르기 표를 못 읽으면 설정 상태의 보기 줄 그대로.
import { afterEach, describe, expect, it, vi } from "vitest";
import { API, SCREEN_PATHS } from "../../web/src/paths";
import { mountSetupExtras } from "../../web/src/setup/extras";
import {
  MODEL_CUSTOM,
  MODELS_APPLY_NOTE,
  MODELS_COST_NOTE,
  MODELS_ERRORS,
  MODELS_REMOTE_NOTE,
  MODELS_SAVED,
} from "../../web/src/setup/models-view";
import type { ModelsReply } from "../../web/src/setup/models-view";

const AGENTS = ["scout", "strategy", "risk", "review", "trader", "leader"];
const BASE = "zz-base-model";
const OTHER = "zz-other-model";

function reply(editable = true, models: Record<string, string> = {}): ModelsReply {
  return {
    roles: AGENTS.map((agent) => ({ agent, name: `이름-${agent}`, model: models[agent] ?? BASE, default: BASE })),
    choices: [
      { id: BASE, label: "기본 모델", note: "기본 · 균형" },
      { id: OTHER, label: "다른 모델", note: "깊음" },
    ],
    editable,
  };
}

function setupStatus(): Record<string, unknown> {
  return {
    items: [],
    ready: true,
    started: true,
    demo: true,
    symbols: ["005930"],
    market: "kr",
    models: AGENTS.map((agent) => ({ agent, model: BASE })),
  };
}

interface Call {
  method: string;
  path: string;
  body: unknown;
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

/** 가짜 서버: 설정 상태와 담당 모델만 답한다. 모델 바꾸기는 정해 둔 답이 없으면 받은 값을 저장한다. */
class FakeServer {
  calls: Call[] = [];
  models: ModelsReply;
  modelsFail = false;
  postReply: ((body: { roles: { agent: string; model: string }[] }) => Response) | null = null;

  constructor(initial: ModelsReply) {
    this.models = initial;
  }

  readonly fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = new URL(String(input), "http://local.test").pathname;
    const method = (init?.method ?? "GET").toUpperCase();
    const body = typeof init?.body === "string" ? JSON.parse(init.body) : null;
    this.calls.push({ method, path, body });
    if (path === "/api/setup/models") {
      if (this.modelsFail) return json(500, { error: "internal" });
      if (method === "GET") return json(200, this.models);
      const sent = body as { roles: { agent: string; model: string }[] };
      if (this.postReply) return this.postReply(sent);
      const picked: Record<string, string> = {};
      for (const row of sent.roles) picked[row.agent] = row.model;
      this.models = reply(true, picked);
      return json(200, this.models);
    }
    if (method === "GET" && path === "/api/setup") return json(200, setupStatus());
    return json(404, { error: "not_found" });
  });

  posts(): Call[] {
    return this.calls.filter((call) => call.method === "POST" && call.path === "/api/setup/models");
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 30; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

async function open(initial: ModelsReply = reply(), prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(initial);
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  document.body.innerHTML = '<main><div id="extras"></div></main>';
  const root = document.getElementById("extras") as HTMLElement;
  await mountSetupExtras(root);
  await settle();
  return root;
}

function box(root: ParentNode): HTMLElement {
  const found = root.querySelector<HTMLElement>('[data-role="models"]');
  expect(found).not.toBeNull();
  return found as HTMLElement;
}

function row(root: ParentNode, agent: string): HTMLElement {
  const found = box(root).querySelector<HTMLElement>(`tr[data-agent="${agent}"]`);
  expect(found, agent).not.toBeNull();
  return found as HTMLElement;
}

function select(root: ParentNode, agent: string): HTMLSelectElement {
  return row(root, agent).querySelector("select") as HTMLSelectElement;
}

function custom(root: ParentNode, agent: string): HTMLInputElement {
  return row(root, agent).querySelector("input") as HTMLInputElement;
}

function action(root: ParentNode, name: string): HTMLButtonElement | null {
  return box(root).querySelector<HTMLButtonElement>(`button[data-action="${name}"]`);
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("설정 화면 · 담당 모델 고르기", () => {
  it("담당 여섯 줄마다 이름 · 고르기(목록 + 직접 입력) · 기본값이 있고 저장 · 되돌리기 단추와 비용 안내가 보인다", async () => {
    // 검증: R18-4 C32 V78
    const root = await open();
    expect(server.calls.some((call) => call.method === "GET" && call.path === API.setupModels)).toBe(true);
    const rows = [...box(root).querySelectorAll<HTMLElement>("[data-agent]")];
    expect(rows.map((tr) => tr.dataset.agent)).toEqual(AGENTS);
    for (const agent of AGENTS) {
      expect(row(root, agent).textContent).toContain(`이름-${agent}`);
      expect(row(root, agent).textContent).toContain(BASE);
      const options = [...select(root, agent).options].map((option) => option.value);
      expect(options).toEqual([BASE, OTHER, MODEL_CUSTOM]);
      expect(select(root, agent).value).toBe(BASE);
      expect(custom(root, agent).hidden, "목록에서 골랐으면 직접 입력 칸은 숨는다").toBe(true);
    }
    expect(action(root, "save-models")).not.toBeNull();
    expect(action(root, "reset-models")).not.toBeNull();
    expect(box(root).textContent).toContain(MODELS_COST_NOTE);
    expect(box(root).textContent).toContain(MODELS_APPLY_NOTE);
  });

  it("목록에 없는 모델이면 직접 입력이 골라져 그 이름이 칸에 있다", async () => {
    // 검증: R18-4 V78
    const root = await open(reply(true, { risk: "zz-custom-model" }));
    expect(select(root, "risk").value).toBe(MODEL_CUSTOM);
    expect(custom(root, "risk").hidden).toBe(false);
    expect(custom(root, "risk").value).toBe("zz-custom-model");
  });

  it("고르고 직접 입력한 뒤 저장하면 여섯 담당을 통째로 보내고 답대로 다시 그리며 저장 안내를 보인다", async () => {
    // 검증: R18-4 C32 V78
    const root = await open();
    select(root, "scout").value = OTHER;
    select(root, "scout").dispatchEvent(new Event("change"));
    select(root, "leader").value = MODEL_CUSTOM;
    select(root, "leader").dispatchEvent(new Event("change"));
    expect(custom(root, "leader").hidden).toBe(false);
    custom(root, "leader").value = " zz-typed-model ";
    action(root, "save-models")?.click();
    await settle();
    expect(server.posts()).toHaveLength(1);
    expect(server.posts()[0].body).toEqual({
      roles: AGENTS.map((agent) => ({
        agent,
        model: agent === "scout" ? OTHER : agent === "leader" ? "zz-typed-model" : BASE,
      })),
    });
    expect(select(root, "scout").value).toBe(OTHER);
    expect(select(root, "leader").value).toBe(MODEL_CUSTOM);
    expect(custom(root, "leader").value).toBe("zz-typed-model");
    expect(box(root).querySelector('[data-role="models-result"]')?.textContent).toContain(MODELS_SAVED);
  });

  it("기본값으로 되돌리기는 여섯 담당을 기본 모델로 보낸다", async () => {
    // 검증: R18-4 C32 V78
    const root = await open(reply(true, { scout: OTHER, trader: "zz-custom" }));
    action(root, "reset-models")?.click();
    await settle();
    expect(server.posts()).toHaveLength(1);
    expect(server.posts()[0].body).toEqual({ roles: AGENTS.map((agent) => ({ agent, model: BASE })) });
    expect(select(root, "scout").value).toBe(BASE);
    expect(select(root, "trader").value).toBe(BASE);
  });

  it("서버가 틀린 값(400)이라 하면 오류 줄을 보이고 고른 값은 그대로 둔다", async () => {
    // 검증: R18-4 C32 V78
    const root = await open(reply(), (fake) => {
      fake.postReply = () => json(400, { error: "invalid_input" });
    });
    select(root, "risk").value = MODEL_CUSTOM;
    select(root, "risk").dispatchEvent(new Event("change"));
    custom(root, "risk").value = "Bad Name";
    action(root, "save-models")?.click();
    await settle();
    const alert = box(root).querySelector('[role="alert"]');
    expect(alert?.textContent).toBe(MODELS_ERRORS.invalid_input);
    expect(custom(root, "risk").value).toBe("Bad Name");
    expect(action(root, "save-models")?.disabled).toBe(false);
  });

  it("다른 기기면 보기만: 고르기 · 칸이 잠기고 단추가 없으며 이 컴퓨터에서만 바꾼다는 안내가 보인다", async () => {
    // 검증: R18-4 C32 V78
    const root = await open(reply(false));
    for (const agent of AGENTS) {
      expect(select(root, agent).disabled).toBe(true);
      expect(custom(root, agent).disabled).toBe(true);
    }
    expect(box(root).querySelectorAll("button").length).toBe(0);
    expect(box(root).textContent).toContain(MODELS_REMOTE_NOTE);
  });

  it("고르기 표를 못 읽으면 설정 상태의 보기 줄(담당 여섯)이 그대로 있다", async () => {
    // 검증: R18-4 V78
    const root = await open(reply(), (fake) => {
      fake.modelsFail = true;
    });
    expect(box(root).querySelectorAll("[data-agent]").length).toBe(6);
    expect(box(root).querySelectorAll("select, button").length).toBe(0);
  });

  it("설정 화면의 경로 표에 담당 모델 경로가 있다", () => {
    // 검증: C32
    expect(API.setupModels).toBe("/api/setup/models");
    expect(SCREEN_PATHS.setup).toContain(API.setupModels);
  });
});
