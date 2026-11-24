// 설정 화면 보강(server 31차 1번 · R18-4 · R16-4 · C32 · V78)을 jsdom에 띄운다(가짜 fetch): 시장 줄과 "다시 켜야 한다"
// 안내, 담당 모델 줄(보기만), 뜻 검색 카드(상태 읽기 → 켜기 · 끄기 → POST 모양 · 기다리는 동안의 표시 · 서버 오류를
// 그대로 보임), 진단 표(상태 배지 · 할 일 줄 · 빈 할 일), 되살리기 안내(명령 글자가 보이고 실행 단추는 없다),
// 알림음 설정(기본 켜짐 · 끄면 기기에 남고 다시 열어도 유지 · 저장소를 못 써도 화면이 죽지 않음), 읽기 실패의 오류 줄,
// 경로 표, 설정 화면이 3D에 닿지 않음.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import { LOAD_FAILED } from "../../web/src/ui/call";
import { SOUND_OFF, SOUND_PREF_KEY } from "../../web/src/ui/sound-pref";
import {
  AGENT_LABELS,
  DIAG_LABELS,
  MARKET_RESTART_NOTE,
  MODELS_NONE,
  MODELS_NOTE,
  NO_TODO,
  RESTORE_COMMAND,
  RESTORE_NOTE,
  SEMANTIC_ERRORS,
  SEMANTIC_REBUILD_NOTE,
  SEMANTIC_SAVING,
  mountSetupExtras,
} from "../../web/src/setup/extras";
import type { DiagnosisItem, DiagnosisReply, ModelRow, SemanticSettings } from "../../web/src/setup/extras";
import { mainOf, reach } from "./source-graph";

const MODEL = "zz-test-model-extras";
/** 진단 상태 → 배지 색(공통 배지의 `data-tone`). */
const TONES: Record<DiagnosisItem["status"], string> = { ok: "success", warn: "warning", fail: "danger", info: "neutral" };
const AGENTS = ["scout", "strategy", "risk", "review", "trader", "leader"];

function models(model = MODEL): ModelRow[] {
  return AGENTS.map((agent) => ({ agent, model }));
}

/** 설정 상태(`GET /api/setup`) 몸통. */
function setupStatus(market: string, rows: ModelRow[] = models()): Record<string, unknown> {
  return {
    items: [
      { name: "broker", required: true, registered: true },
      { name: "ai", required: true, registered: true },
      { name: "symbols", required: true, registered: true },
      { name: "data", required: false, registered: false },
    ],
    ready: true,
    started: true,
    demo: true,
    symbols: ["005930"],
    symbols_source: "saved",
    ai: { method: "api_key", command: null, config_dir: null, cli: null, cli_version: null, subscription: null, legacy_auth_file: false },
    market,
    models: rows,
  };
}

const SEMANTIC_OFF: SemanticSettings = { enabled: false, base_url: "http://127.0.0.1:11434", model: "nomic-embed-text" };

function diagItem(key: string, status: DiagnosisItem["status"], todo: string | null): DiagnosisItem {
  return { key, label: `이름-${key}`, status, detail: `본 것-${key}`, todo };
}

const DIAGNOSIS: DiagnosisReply = {
  items: [
    diagItem("store", "ok", null),
    diagItem("keys", "warn", "설정 화면에서 키를 등록합니다"),
    diagItem("ai", "info", null),
    diagItem("backup", "fail", "백업 칸에서 토큰을 확인합니다"),
    diagItem("market", "info", null),
  ],
};

interface Call {
  method: string;
  url: string;
  path: string;
  body: unknown;
}

/** 가짜 서버: 설정 상태 · 뜻 검색 · 진단 · 백업을 들고 답한다. 뜻 검색 바꾸기는 정해 둔 답을 낸다. */
class FakeServer {
  calls: Call[] = [];
  fail = new Set<string>();
  semantic: SemanticSettings = { ...SEMANTIC_OFF };
  backup: unknown = { repo: null, recipient: null, configured: false };
  /** 뜻 검색 `POST`의 답. 없으면 받은 값을 그대로 저장하고 200. */
  semanticReply: ((body: SemanticSettings) => Promise<Response> | Response) | null = null;

  constructor(
    public status: Record<string, unknown>,
    public diagnosis: DiagnosisReply,
  ) {}

  readonly fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = (init?.method ?? "GET").toUpperCase();
    const path = new URL(url, "http://local.test").pathname;
    const body = typeof init?.body === "string" ? JSON.parse(init.body) : null;
    this.calls.push({ method, url, path, body });
    if (this.fail.has(path)) return json(500, { error: "internal" });
    if (method === "POST" && path === "/api/session") return json(200, {});
    if (method === "POST" && path === "/api/setup/semantic") {
      if (this.semanticReply) return this.semanticReply(body as SemanticSettings);
      this.semantic = body as SemanticSettings;
      return json(200, this.semantic);
    }
    if (method !== "GET") return json(404, { error: "not_found" });
    if (path === "/api/setup") return json(200, this.status);
    if (path === "/api/setup/semantic") return json(200, this.semantic);
    if (path === "/api/setup/doctor") return json(200, this.diagnosis);
    if (path === "/api/setup/backup") return json(200, this.backup);
    return json(404, { error: "not_found" });
  });

  count(method: string, path: string): number {
    return this.calls.filter((call) => call.method === method && call.path === path).length;
  }
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 자리 하나에 보강 묶음을 띄운다. */
async function open(prepare?: (fake: FakeServer) => void, status = setupStatus("kr")): Promise<HTMLElement> {
  server = new FakeServer(status, DIAGNOSIS);
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  document.body.innerHTML = '<main><h1>가격인하</h1><div id="extras"></div></main>';
  const root = document.getElementById("extras") as HTMLElement;
  await mountSetupExtras(root);
  await settle();
  return root;
}

function pick<T extends Element = HTMLElement>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

function text(root: ParentNode, selector: string): string {
  return pick(root, selector).textContent ?? "";
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
  try {
    localStorage.clear();
  } catch {
    // 저장소를 바꿔 둔 시험은 스스로 되돌린다.
  }
});

describe("설정 보강 · 시장 · 담당 모델", () => {
  it("설정 상태를 읽어 시장(국내)과 '다시 켜야 한다' 안내를 보이고 바꾸는 칸은 없다", async () => {
    // 검증: R18-4 V78 C32
    const root = await open();
    expect(server.count("GET", "/api/setup")).toBeGreaterThan(0);
    const market = pick(root, '[data-role="market"]');
    expect(market.textContent).toContain("국내");
    expect(market.textContent).toContain(MARKET_RESTART_NOTE);
    expect(market.textContent).toContain("다시 켜야");
    expect(market.querySelectorAll("input, select, button").length, "시장은 보기만").toBe(0);
  });

  it("미국 시장이면 '미국'을 보인다", async () => {
    // 검증: R18-4 V78
    const root = await open(undefined, setupStatus("us"));
    expect(text(root, '[data-role="market"]')).toContain("미국");
    expect(text(root, '[data-role="market"]')).not.toContain("국내");
  });

  it("담당마다 이름과 지금 쓰는 모델 이름을 보이고, 고르는 칸 · 단추는 없다(보기만)", async () => {
    // 검증: R18-4 V78 C32
    const root = await open();
    const box = pick(root, '[data-role="models"]');
    const rows = [...box.querySelectorAll<HTMLElement>("[data-agent]")];
    expect(rows.map((row) => row.dataset.agent)).toEqual(AGENTS);
    for (const row of rows) {
      expect(row.textContent).toContain(MODEL);
      expect(row.textContent).toContain(AGENT_LABELS[row.dataset.agent as string]);
    }
    expect(box.textContent).toContain(MODELS_NOTE);
    expect(box.querySelectorAll("input, select, button").length, "모델은 보기만").toBe(0);
  });

  it("모델 목록이 비면 빈 상태를 보인다", async () => {
    // 검증: R18-4 V78
    const root = await open(undefined, setupStatus("kr", []));
    const box = pick(root, '[data-role="models"]');
    expect(box.querySelectorAll("[data-agent]").length).toBe(0);
    expect(box.textContent).toContain(MODELS_NONE);
  });
});

describe("설정 보강 · 뜻 검색", () => {
  it("상태를 읽어 꺼짐 · 주소 · 모델을 보이고 단추는 '켜기'다", async () => {
    // 검증: R18-4 V78 C32
    const root = await open();
    expect(server.count("GET", "/api/setup/semantic")).toBeGreaterThan(0);
    const card = pick(root, '[data-role="semantic"]');
    expect(text(card, '[data-role="semantic-state"]')).toContain("꺼짐");
    expect(pick<HTMLInputElement>(card, "#semantic-url").value).toBe(SEMANTIC_OFF.base_url);
    expect(pick<HTMLInputElement>(card, "#semantic-model").value).toBe(SEMANTIC_OFF.model);
    expect(text(card, 'button[data-action="semantic-toggle"]')).toContain("켜기");
  });

  it("켜기를 누르면 주소 · 모델 칸의 값으로 enabled=true를 보내고, 답대로 켜짐과 다시 만들기 안내를 보인다", async () => {
    // 검증: R18-4 V78 C32
    const root = await open();
    const card = pick(root, '[data-role="semantic"]');
    pick<HTMLInputElement>(card, "#semantic-url").value = "http://localhost:11500";
    pick<HTMLInputElement>(card, "#semantic-model").value = "zz-embed";
    pick<HTMLButtonElement>(card, 'button[data-action="semantic-toggle"]').click();
    await settle();
    const posts = server.calls.filter((call) => call.method === "POST" && call.path === "/api/setup/semantic");
    expect(posts.length).toBe(1);
    expect(posts[0].body).toEqual({ enabled: true, base_url: "http://localhost:11500", model: "zz-embed" });
    const after = pick(root, '[data-role="semantic"]');
    expect(text(after, '[data-role="semantic-state"]')).toContain("켜짐");
    expect(text(after, 'button[data-action="semantic-toggle"]')).toContain("끄기");
    expect(after.textContent).toContain(SEMANTIC_REBUILD_NOTE);
  });

  it("켜져 있으면 끄기를 누를 때 enabled=false와 지금 주소 · 모델을 보내고 꺼짐을 보인다", async () => {
    // 검증: R18-4 V78
    const root = await open((fake) => {
      fake.semantic = { ...SEMANTIC_OFF, enabled: true };
    });
    const card = pick(root, '[data-role="semantic"]');
    expect(text(card, '[data-role="semantic-state"]')).toContain("켜짐");
    pick<HTMLButtonElement>(card, 'button[data-action="semantic-toggle"]').click();
    await settle();
    const posts = server.calls.filter((call) => call.method === "POST" && call.path === "/api/setup/semantic");
    expect(posts.map((call) => call.body)).toEqual([{ ...SEMANTIC_OFF, enabled: false }]);
    expect(text(root, '[data-role="semantic"] [data-role="semantic-state"]')).toContain("꺼짐");
  });

  it("보내고 답을 기다리는 동안 단추가 잠기고 '저장하는 중'이 보이며, 답이 오면 풀린다", async () => {
    // 검증: V78 R18-4
    let answer: (reply: Response) => void = () => {};
    const root = await open((fake) => {
      fake.semanticReply = (body) =>
        new Promise<Response>((resolve) => {
          answer = resolve;
          fake.semantic = body;
        });
    });
    pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').click();
    await settle();
    const busy = pick(root, '[data-role="semantic"] [aria-busy="true"]');
    expect(busy.textContent).toContain(SEMANTIC_SAVING);
    expect(pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').disabled).toBe(true);
    pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').click();
    await settle();
    expect(server.count("POST", "/api/setup/semantic"), "기다리는 동안 다시 보내지 않는다").toBe(1);
    answer(json(200, { ...SEMANTIC_OFF, enabled: true }));
    await settle();
    expect(root.querySelector('[data-role="semantic"] [aria-busy="true"]')).toBeNull();
    expect(pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').disabled).toBe(false);
    expect(text(root, '[data-role="semantic"] [data-role="semantic-state"]')).toContain("켜짐");
  });

  it("같은 컴퓨터 주소가 아니어도 화면이 막지 않고 보내며, 서버의 거절(loopback_only)을 오류 줄로 보이고 상태는 그대로다", async () => {
    // 검증: V78 R18-4 E57
    const root = await open((fake) => {
      fake.semanticReply = () => json(400, { error: "loopback_only" });
    });
    pick<HTMLInputElement>(root, "#semantic-url").value = "http://192.168.0.9:11434";
    pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').click();
    await settle();
    expect(server.count("POST", "/api/setup/semantic"), "주소를 화면이 미리 거르지 않는다").toBe(1);
    const alert = pick(root, '[data-role="semantic"] [role="alert"]');
    expect(alert.textContent).toContain(SEMANTIC_ERRORS.loopback_only);
    expect(text(root, '[data-role="semantic"] [data-role="semantic-state"]')).toContain("꺼짐");
    expect(pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').disabled).toBe(false);
  });

  it("모르는 까닭이면 서버가 준 error 글자를 그대로 보인다", async () => {
    // 검증: V78 E57
    const root = await open((fake) => {
      fake.semanticReply = () => json(500, { error: "zz_unknown_reason" });
    });
    pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').click();
    await settle();
    expect(text(root, '[data-role="semantic"] [role="alert"]')).toContain("zz_unknown_reason");
  });
});

describe("설정 보강 · 진단", () => {
  it("진단을 읽어 항목마다 이름 · 상태 배지 · 본 것 · 할 일 줄을 보이고, 할 일이 없으면 '할 일 없음'이다", async () => {
    // 검증: V78 C32 R18-4
    const root = await open();
    expect(server.count("GET", "/api/setup/doctor")).toBeGreaterThan(0);
    const table = pick(root, '[data-role="diagnosis"]');
    const rows = [...table.querySelectorAll<HTMLElement>("tr[data-key]")];
    expect(rows.map((row) => row.dataset.key)).toEqual(DIAGNOSIS.items.map((item) => item.key));
    for (const item of DIAGNOSIS.items) {
      const row = pick(table, `tr[data-key="${item.key}"]`);
      expect(row.textContent).toContain(item.label);
      expect(row.textContent).toContain(item.detail);
      const badge = pick(row, ".ui-badge");
      expect(badge.textContent).toBe(DIAG_LABELS[item.status]);
      expect(badge.dataset.tone, "정상 · 살펴볼 것 · 문제 · 알림마다 다른 색").toBe(TONES[item.status]);
      expect(text(row, '[data-role="todo"]')).toBe(item.todo ?? NO_TODO);
    }
  });

  it("진단을 못 읽으면 그 자리에 오류 줄이 보이고 다른 자리는 그대로 그린다", async () => {
    // 검증: V78 E57
    const root = await open((fake) => fake.fail.add("/api/setup/doctor"));
    expect(text(root, '[data-role="diagnosis"] [role="alert"]')).toContain(LOAD_FAILED);
    expect(text(root, '[data-role="market"]')).toContain("국내");
    expect(root.querySelector('[data-role="semantic"]')).not.toBeNull();
    expect(root.querySelector('[data-role="sound"]')).not.toBeNull();
  });

  it("설정 상태를 못 읽으면 시장 · 모델 자리에 오류 줄이 보이고 진단은 그대로 그린다", async () => {
    // 검증: V78 E57
    const root = await open((fake) => fake.fail.add("/api/setup"));
    expect(text(root, '[data-role="market"] [role="alert"]')).toContain(LOAD_FAILED);
    expect(root.querySelectorAll('[data-role="diagnosis"] tr[data-key]').length).toBe(DIAGNOSIS.items.length);
  });

  it("뜻 검색 설정을 못 읽으면 그 카드에 오류 줄이 보이고 켜기 단추는 없다", async () => {
    // 검증: V78 E57
    const root = await open((fake) => fake.fail.add("/api/setup/semantic"));
    expect(text(root, '[data-role="semantic"] [role="alert"]')).toContain(LOAD_FAILED);
    expect(root.querySelector('[data-role="semantic"] button[data-action="semantic-toggle"]')).toBeNull();
  });
});

describe("설정 보강 · 되살리기 안내", () => {
  it("백업 상태와 price-inha restore 명령 쓰는 법이 보이고, 되살리기를 실행하는 단추는 없다", async () => {
    // 검증: R16-4 V78
    const root = await open((fake) => {
      fake.backup = { repo: "zz-owner/zz-backup", recipient: "age1zzzz", configured: true };
    });
    expect(server.count("GET", "/api/setup/backup")).toBeGreaterThan(0);
    const guide = pick(root, '[data-role="restore"]');
    expect(text(guide, '[data-role="backup-state"]')).toContain("켜짐");
    expect(text(guide, "code").startsWith(RESTORE_COMMAND)).toBe(true);
    expect(guide.textContent).toContain(RESTORE_NOTE);
    expect(guide.querySelectorAll("button").length, "되살리기 단추 없음").toBe(0);
    const restoreButtons = [...root.querySelectorAll("button")].filter((b) => (b.textContent ?? "").includes("되살리"));
    expect(restoreButtons).toEqual([]);
    expect(server.calls.some((call) => call.path.includes("restore")), "되살리기 경로를 부르지 않는다").toBe(false);
  });

  it("백업이 꺼져 있으면 꺼짐을 보이고 명령 안내는 그대로 있다", async () => {
    // 검증: R16-4 V78
    const root = await open();
    const guide = pick(root, '[data-role="restore"]');
    expect(text(guide, '[data-role="backup-state"]')).toContain("꺼짐");
    expect(text(guide, "code")).toContain(RESTORE_COMMAND);
  });
});

describe("설정 보강 · 알림음", () => {
  it("기본은 켜짐이고, 끄면 기기에 남아 다시 열어도 꺼짐이다", async () => {
    // 검증: V78 R18-4
    localStorage.clear();
    let root = await open();
    const box = pick<HTMLInputElement>(root, '[data-role="sound"] input#sound-on');
    expect(box.type).toBe("checkbox");
    expect(box.checked, "기본은 켜짐").toBe(true);
    box.click();
    await settle();
    expect(localStorage.getItem(SOUND_PREF_KEY)).toBe(SOUND_OFF);
    expect(server.calls.filter((call) => call.method === "POST").length, "서버에 보내지 않는다").toBe(0);
    root = await open();
    expect(pick<HTMLInputElement>(root, "input#sound-on").checked, "다시 열어도 꺼짐").toBe(false);
  });

  it("저장소를 쓸 수 없는 브라우저에서도 화면이 그려지고 켜짐으로 보이며, 눌러도 죽지 않는다", async () => {
    // 검증: V78 E57
    vi.stubGlobal("localStorage", {
      getItem(): string | null {
        throw new DOMException("막힘", "SecurityError");
      },
      setItem(): void {
        throw new DOMException("막힘", "SecurityError");
      },
      removeItem(): void {},
      clear(): void {},
      key(): string | null {
        return null;
      },
      length: 0,
    });
    const root = await open();
    const box = pick<HTMLInputElement>(root, "input#sound-on");
    expect(box.checked).toBe(true);
    expect(() => box.click()).not.toThrow();
    await settle();
    expect(root.querySelector('[data-role="diagnosis"]')).not.toBeNull();
  });
});

describe("설정 보강 · 경로 · 가벼움", () => {
  it("부르는 경로는 모두 설정 화면의 경로 표에 있고 바깥 주소가 없다", async () => {
    // 검증: D11 D2 C32
    const root = await open();
    pick<HTMLButtonElement>(root, '[data-role="semantic"] button[data-action="semantic-toggle"]').click();
    await settle();
    const seen = new Set(server.calls.map((call) => call.path));
    for (const path of seen) expect(SCREEN_PATHS.setup, path).toContain(path);
    for (const path of ["/api/setup", "/api/setup/semantic", "/api/setup/doctor", "/api/setup/backup"]) {
      expect(seen.has(path), `${path}을 부른다`).toBe(true);
    }
    expect(server.calls.every((call) => !call.url.includes("://"))).toBe(true);
  });

  it("설정 화면 소스는 보강 묶음 · 알림음 설정 · 공통 부품에 닿고 three · 사무실 3D에는 닿지 않는다", () => {
    // 검증: V77 N21 V78
    const { files, bare } = reach(mainOf("setup"));
    expect(files.has("../../web/src/setup/extras.ts"), "보강 묶음을 단다").toBe(true);
    expect(files.has("../../web/src/ui/sound-pref.ts"), "알림음 설정을 쓴다").toBe(true);
    expect(files.has("../../web/src/ui/parts.ts"), "공통 부품을 쓴다").toBe(true);
    expect([...bare].filter((name) => name === "three" || name.startsWith("three/"))).toEqual([]);
    expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/"))).toEqual([]);
  });
});
