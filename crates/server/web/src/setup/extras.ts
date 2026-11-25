// 설정 화면 보강(server 31차 1번 · 스펙 R18-4 · R16-4 · 계약 C32 · 검증 V78): 시장 · 담당 모델 줄, 뜻 검색 카드,
// 진단 표, 되살리기 안내, 알림음 설정. 설정 화면(`setup/main.ts`)이 [`mountSetupExtras`]로 자리 하나에 단다.
// 공통 부품(`ui/parts.ts`) · 같은 출처 부르기(`ui/call.ts`)만 쓰고 3D에 닿지 않는다. 서버가 준 글자는 글자로만 넣는다.
//
// 읽는 경로(모두 `GET`, 경로 표 `SCREEN_PATHS.setup` 안): 설정 상태 `API.setup`(`market` · `models` 칸),
// 뜻 검색 `API.setupSemantic`, 진단 `API.setupDoctor`, 백업 `API.setupBackup`, 알림음 `API.setupSound`. 바꾸는 경로는
// 뜻 검색 `POST`와 알림음 `POST`뿐이다(시장 · 모델은 보기만, 되살리기는 화면에서 하지 않는다 — 실행 중인 저장소를
// 바꾸지 않는다).
//
// 그리는 자리(`data-role`):
// - `market`: 시장 이름(`kr` → "국내", `us` → "미국")과 [`MARKET_RESTART_NOTE`]. 입력 · 단추 없음.
// - `models`: 담당마다 `[data-agent=<id>]` 줄(담당 이름 [`AGENT_LABELS`] · 모델 이름)과 [`MODELS_NOTE`].
//   목록이 비면 빈 상태 [`MODELS_NONE`]. 입력 · 고르기 · 단추 없음(보기만).
// - `semantic`: 상태 `[data-role="semantic-state"]`("켜짐" · "꺼짐"), 주소 칸 `#semantic-url`, 모델 칸 `#semantic-model`,
//   단추 `button[data-action="semantic-toggle"]`(꺼져 있으면 "켜기", 켜져 있으면 "끄기"), 안내 [`SEMANTIC_LOOPBACK_NOTE`].
//   누르면 `POST API.setupSemantic` `{"enabled": 반대, "base_url": 주소 칸, "model": 모델 칸}`을 보낸다(주소를 화면이
//   미리 거르지 않는다). 기다리는 동안 단추는 잠기고 불러오는 중 줄([`SEMANTIC_SAVING`], `aria-busy="true"`)이 보인다.
//   200이면 답의 값으로 다시 그리고 켜졌으면 [`SEMANTIC_REBUILD_NOTE`]를 보인다. 실패면 오류 줄(`role="alert"`)에
//   [`SEMANTIC_ERRORS`]의 글(모르는 까닭이면 서버가 준 `error` 글자 그대로)을 보이고 상태는 그대로 둔다.
// - `diagnosis`: 진단 표(공통 표). 항목마다 `tr[data-key=<key>]` — 이름 · 상태 배지(`.ui-badge`, 글 [`DIAG_LABELS`],
//   `data-tone` [`DIAG_TONES`]) · 무엇을 봤나 · 할 일 `[data-role="todo"]`(`null`이면 [`NO_TODO`]).
// - `restore`: 백업 상태 `[data-role="backup-state"]`("백업 켜짐" · "백업 꺼짐"), 명령 `code`([`RESTORE_COMMAND`]로
//   시작한다)와 [`RESTORE_NOTE`]. 되살리기를 실행하는 단추는 없다.
// - `sound`: 체크 칸 `#sound-on`(이름 [`SOUND_LABEL`]) — `ui/sound-pref.ts`로 서버(`API.setupSound`)에서 읽고 쓴다
//   (기본 켜짐, 31차 고침 · 결정 22). [`soundSetting`] 참고.
//
// 읽기 실패: 그 경로를 쓰는 자리만 오류 줄(`role="alert"`, `LOAD_FAILED`)을 보이고 다른 자리는 그대로 그린다.
//
// (39차 7번) 메신저 묶음 `messenger`: 자리 하나를 더 두고 `setup/messenger-view.ts`의 `mountMessenger`로 그린다
// (`GET API.messenger`, 계약은 그 파일).
import { API } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { badge, card, emptyState, errorLine, field, loading, table, button } from "../ui/parts";
import { mountMessenger } from "./messenger-view";
import type { BadgeTone } from "../ui/parts";
import { SOUND_DEFAULT, saveSound, soundEnabled } from "../ui/sound-pref";

/** 진단 상태(서버 `DIAG_*`). */
export type DiagStatus = "ok" | "warn" | "fail" | "info";

/** 진단 항목 하나(`GET /api/setup/doctor`의 `items` 한 줄). */
export interface DiagnosisItem {
  key: string;
  label: string;
  status: DiagStatus;
  detail: string;
  todo: string | null;
}

/** 진단 답. */
export interface DiagnosisReply {
  items: DiagnosisItem[];
}

/** 뜻 검색 설정(`GET` · `POST /api/setup/semantic`의 몸통 · 답). */
export interface SemanticSettings {
  enabled: boolean;
  base_url: string;
  model: string;
}

/** 담당 모델 한 줄(설정 상태의 `models`). */
export interface ModelRow {
  agent: string;
  model: string;
}

/** 시장 글자 → 화면 이름. */
export const MARKET_LABELS: Readonly<Record<string, string>> = { kr: "국내", us: "미국" };

/** 시장 줄의 안내. */
export const MARKET_RESTART_NOTE = "시장은 켤 때 정합니다. 바꾸려면 --market 인자를 바꿔 서버를 다시 켜야 합니다.";

/** 담당 id → 화면 이름. */
export const AGENT_LABELS: Readonly<Record<string, string>> = {
  scout: "뉴스 · 공시 · 시세",
  strategy: "전략",
  risk: "리스크",
  review: "복기",
  trader: "매매",
  leader: "팀장",
};

/** 모델 줄의 안내(보기만). */
export const MODELS_NOTE = "담당 모델은 지금 보기만 합니다. 바꾸는 길은 아직 없습니다.";

/** 모델 목록이 비었을 때(팀을 잇지 않은 서버). */
export const MODELS_NONE = "팀이 이어져 있지 않아 쓰는 모델이 없습니다.";

/** 뜻 검색 주소 안내. */
export const SEMANTIC_LOOPBACK_NOTE = "Ollama 주소는 같은 컴퓨터(127.0.0.1 · localhost)만 됩니다.";

/** 뜻 검색을 바꾸는 동안의 줄. */
export const SEMANTIC_SAVING = "저장하는 중";

/** 뜻 검색을 켠 뒤의 안내(다시 만들기는 한가할 때 서버가 한다). */
export const SEMANTIC_REBUILD_NOTE = "켜졌습니다. 기존 기록의 뜻 색인은 한가할 때 조금씩 다시 만듭니다.";

/** 뜻 검색 바꾸기 실패 까닭 → 화면 글. */
export const SEMANTIC_ERRORS: Readonly<Record<string, string>> = {
  loopback_only: "Ollama 주소는 같은 컴퓨터(127.0.0.1 · localhost)만 됩니다. 주소를 확인하세요.",
  invalid_input: "주소와 모델 이름을 모두 적어 주세요.",
  local_only: "뜻 검색 설정은 이 컴퓨터에서만 바꿀 수 있습니다.",
};

/** 진단 상태 → 배지 글. */
export const DIAG_LABELS: Readonly<Record<DiagStatus, string>> = { ok: "정상", warn: "살펴볼 것", fail: "문제", info: "알림" };

/** 진단 상태 → 배지 색. */
export const DIAG_TONES: Readonly<Record<DiagStatus, BadgeTone>> = {
  ok: "success",
  warn: "warning",
  fail: "danger",
  info: "neutral",
};

/** 할 일이 없을 때 할 일 칸의 글. */
export const NO_TODO = "할 일 없음";

/** 되살리기 명령(앞부분). */
export const RESTORE_COMMAND = "price-inha restore";

/** 되살리기 안내. */
export const RESTORE_NOTE =
  "되살리기는 이 화면에서 하지 않습니다. 서버를 끈 뒤 터미널에서 아래 명령으로 빈 폴더에 되살리고, 그 폴더로 다시 켭니다.";

/** 알림음 칸 이름. */
export const SOUND_LABEL = "승인 알림음";

/** 알림음 바꾸기에 실패했을 때의 글. */
const SOUND_SAVE_FAILED = "알림음 설정을 바꾸지 못했습니다. 서버와 연결되지 않았거나 잠시 응답하지 않습니다. 다시 시도해 주세요.";

/** 되살린 폴더로 다시 켜는 명령. */
const RESTART_COMMAND = "price-inha run --home <되살린 폴더>";

/** 되살리기 명령 전체(파일 · 폴더 자리는 사람이 채운다). */
const RESTORE_USAGE = `${RESTORE_COMMAND} <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>`;

/** 객체면 그대로, 아니면 빈 객체. */
function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" ? (value as Record<string, unknown>) : {};
}

/** 자리 하나(공통 카드)에 `data-role`을 단다. */
function block(role: string, title: string, ...body: Node[]): HTMLElement {
  const node = card(title, ...body);
  node.dataset.role = role;
  node.classList.add("setup-extra");
  return node;
}

/** 읽기에 실패한 자리. */
function failedBlock(role: string, title: string): HTMLElement {
  return block(role, title, errorLine(LOAD_FAILED));
}

/** 설정 상태 몸통으로 시장 줄을 그린다(`data-role="market"`). */
export function marketLine(status: unknown): HTMLElement {
  const market = String(record(status).market ?? "");
  const name = MARKET_LABELS[market] ?? market;
  return block("market", "시장", el("p", "setup-extra-value", `${name} (${market})`), el("p", "hint", MARKET_RESTART_NOTE));
}

/** 설정 상태의 `models`에서 담당 · 모델 글자가 있는 줄만. */
function modelRows(status: unknown): ModelRow[] {
  const models = record(status).models;
  if (!Array.isArray(models)) return [];
  return models
    .map(record)
    .filter((row) => typeof row.agent === "string" && typeof row.model === "string")
    .map((row) => ({ agent: row.agent as string, model: row.model as string }));
}

/** 설정 상태 몸통으로 담당 모델 줄을 그린다(`data-role="models"`). */
export function modelsLine(status: unknown): HTMLElement {
  const rows = modelRows(status);
  if (rows.length === 0) return block("models", "담당 모델", emptyState(MODELS_NONE), el("p", "hint", MODELS_NOTE));
  const list = el("ul", "setup-extra-list");
  for (const row of rows) {
    const item = el("li", "setup-extra-row");
    item.dataset.agent = row.agent;
    item.append(el("span", "setup-extra-name", AGENT_LABELS[row.agent] ?? row.agent), el("code", "setup-extra-code", row.model));
    list.append(item);
  }
  return block("models", "담당 모델", list, el("p", "hint", MODELS_NOTE));
}

/** 진단 상태 글자면 그대로, 아니면 알림. */
function diagStatus(value: unknown): DiagStatus {
  return value === "ok" || value === "warn" || value === "fail" ? value : "info";
}

/** 진단 답의 항목들(모양이 맞지 않는 칸은 글자로 바꾼다). */
function diagnosisItems(reply: unknown): DiagnosisItem[] {
  const items = record(reply).items;
  if (!Array.isArray(items)) return [];
  return items.map(record).map((item) => ({
    key: String(item.key ?? ""),
    label: String(item.label ?? ""),
    status: diagStatus(item.status),
    detail: String(item.detail ?? ""),
    todo: typeof item.todo === "string" ? item.todo : null,
  }));
}

/** 진단 답으로 진단 표를 그린다(`data-role="diagnosis"`). */
export function diagnosisTable(reply: unknown): HTMLElement {
  const items = diagnosisItems(reply);
  const rows = items.map((item) => [item.label, "", item.detail, item.todo ?? NO_TODO]);
  const grid = table(["항목", "상태", "본 것", "할 일"], rows);
  const lines = grid.tBodies[0]?.rows ?? [];
  items.forEach((item, index) => {
    const tr = lines[index];
    if (!tr) return;
    tr.dataset.key = item.key;
    tr.cells[1]?.append(badge(DIAG_LABELS[item.status], DIAG_TONES[item.status]));
    tr.cells[3]?.setAttribute("data-role", "todo");
  });
  const wrap = el("div", "setup-extra-wrap");
  wrap.append(grid);
  return block("diagnosis", "진단", wrap, el("p", "hint", "실행 중인 서버가 아는 것만 봅니다. 더 자세히는 서버를 끈 뒤 price-inha doctor 를 실행합니다."));
}

/** 백업 설정 답(`{"repo", "recipient", "configured"}`)으로 되살리기 안내를 그린다(`data-role="restore"`). */
export function restoreGuide(backup: unknown): HTMLElement {
  const on = record(backup).configured === true;
  const state = el("p", "setup-extra-value", on ? "백업 켜짐" : "백업 꺼짐");
  state.dataset.role = "backup-state";
  const usage = el("pre", "setup-extra-command");
  usage.append(el("code", "", RESTORE_USAGE));
  const restart = el("pre", "setup-extra-command");
  restart.append(el("code", "", RESTART_COMMAND));
  return block("restore", "되살리기", state, el("p", "hint", RESTORE_NOTE), usage, restart);
}

/** 답이 뜻 검색 설정 모양이면 그 값. */
function semanticOf(value: unknown): SemanticSettings | null {
  const body = record(value);
  if (typeof body.enabled !== "boolean" || typeof body.base_url !== "string" || typeof body.model !== "string") return null;
  return { enabled: body.enabled, base_url: body.base_url, model: body.model };
}

/** 뜻 검색 바꾸기 실패 까닭 → 화면 글. */
function semanticError(body: unknown): string {
  const reason = record(body).error;
  if (typeof reason !== "string" || reason === "") return LOAD_FAILED;
  return SEMANTIC_ERRORS[reason] ?? reason;
}

/** 칸 묶음 안의 입력 칸. */
function inputOf(holder: HTMLElement): HTMLInputElement {
  return holder.querySelector("input") as HTMLInputElement;
}

/** 뜻 검색 카드를 그린다(`data-role="semantic"`). 단추를 누르면 바꾸기를 보낸다. */
export function semanticCard(initial: SemanticSettings): HTMLElement {
  let current = initial;
  let busy = false;
  const state = el("p", "setup-extra-value");
  state.dataset.role = "semantic-state";
  const urlField = field("Ollama 주소", "semantic-url", "url");
  const modelField = field("임베딩 모델", "semantic-model");
  inputOf(urlField).value = current.base_url;
  inputOf(modelField).value = current.model;
  const toggle = button("", "accent");
  toggle.dataset.action = "semantic-toggle";
  const message = el("div", "setup-extra-message");
  message.setAttribute("aria-live", "polite");
  const paint = (): void => {
    state.textContent = current.enabled ? "켜짐" : "꺼짐";
    toggle.textContent = current.enabled ? "끄기" : "켜기";
  };
  toggle.addEventListener("click", () => {
    if (busy) return;
    busy = true;
    toggle.disabled = true;
    message.replaceChildren(loading(SEMANTIC_SAVING));
    const body = { enabled: !current.enabled, base_url: inputOf(urlField).value, model: inputOf(modelField).value };
    void call("POST", API.setupSemantic, body)
      .then((reply) => {
        const next = reply.ok ? semanticOf(reply.body) : null;
        if (!next) return message.replaceChildren(errorLine(semanticError(reply.body)));
        current = next;
        inputOf(urlField).value = next.base_url;
        inputOf(modelField).value = next.model;
        paint();
        message.replaceChildren(...(next.enabled ? [el("p", "hint", SEMANTIC_REBUILD_NOTE)] : []));
      })
      .catch(() => message.replaceChildren(errorLine(LOAD_FAILED)))
      .finally(() => {
        busy = false;
        toggle.disabled = false;
      });
  });
  paint();
  return block("semantic", "뜻 검색", state, urlField, modelField, el("p", "hint", SEMANTIC_LOOPBACK_NOTE), toggle, message);
}

/**
 * 알림음 설정을 그린다(`data-role="sound"`, 31차 고침 · 결정 22). 체크 칸 `#sound-on`은 서버에서 읽은 값
 * (`ui/sound-pref.ts`, 읽기 실패면 켜짐)을 보인다. 바꾸면 `POST API.setupSound`를 보내고, 기다리는 동안 칸을 잠그며,
 * 서버의 답으로 맞춘다. 실패면 칸을 되돌리고 이 자리에 오류 줄(`role="alert"`)을 보인다. 브라우저에 남기지 않는다.
 */
export function soundSetting(): HTMLElement {
  return soundBlock().node;
}

/** [`soundSetting`]의 자리와, 서버에서 읽은 값을 칸에 보이고 나면 끝나는 약속(던지지 않는다). */
function soundBlock(): { node: HTMLElement; ready: Promise<void> } {
  const holder = field(SOUND_LABEL, "sound-on", "checkbox");
  holder.classList.add("setup-extra-check");
  const box = inputOf(holder);
  box.checked = SOUND_DEFAULT;
  // 서버 값을 읽기 전에는 바꾸지 못하게 잠근다.
  box.disabled = true;
  const message = el("div", "setup-extra-message");
  message.setAttribute("aria-live", "polite");
  let current = SOUND_DEFAULT;
  box.addEventListener("change", () => {
    const wanted = box.checked;
    box.disabled = true;
    message.replaceChildren();
    void saveSound(wanted).then((saved) => {
      if (saved === null) message.replaceChildren(errorLine(SOUND_SAVE_FAILED));
      else current = saved;
      box.checked = current;
      box.disabled = false;
    });
  });
  const ready = soundEnabled().then((on) => {
    current = on;
    box.checked = on;
    box.disabled = false;
  });
  return { node: block("sound", "알림음", holder, message), ready };
}

/** 경로 하나를 읽는다. 실패(상태 · 연결)면 `null`. */
async function read(url: string): Promise<unknown> {
  try {
    const reply = await call("GET", url);
    return reply.ok ? reply.body : null;
  } catch {
    return null;
  }
}

/** 자리를 새것으로 바꾼다. */
function swap(slot: HTMLElement, next: HTMLElement): HTMLElement {
  slot.replaceWith(next);
  return next;
}

/** 자리 하나에 보강 묶음 모두를 읽어 그린다. 읽기가 실패해도 던지지 않는다. */
export async function mountSetupExtras(root: HTMLElement): Promise<void> {
  const titles: Record<string, string> = { market: "시장", models: "담당 모델", semantic: "뜻 검색", diagnosis: "진단", restore: "되살리기" };
  const slots: Record<string, HTMLElement> = {};
  for (const [role, title] of Object.entries(titles)) slots[role] = block(role, title, loading());
  const sound = soundBlock();
  // (39차 7번) 메신저 묶음은 제 자리를 스스로 읽어 그린다.
  const messengerSlot = el("div", "");
  root.replaceChildren(...Object.values(slots), sound.node, messengerSlot);
  const messenger = mountMessenger(messengerSlot);
  const status = read(API.setup).then((body) => {
    swap(slots.market, body === null ? failedBlock("market", titles.market) : marketLine(body));
    swap(slots.models, body === null ? failedBlock("models", titles.models) : modelsLine(body));
  });
  const semantic = read(API.setupSemantic).then((body) => {
    const settings = semanticOf(body);
    swap(slots.semantic, settings === null ? failedBlock("semantic", titles.semantic) : semanticCard(settings));
  });
  const doctor = read(API.setupDoctor).then((body) => {
    swap(slots.diagnosis, body === null ? failedBlock("diagnosis", titles.diagnosis) : diagnosisTable(body));
  });
  const backup = read(API.setupBackup).then((body) => {
    const guide = swap(slots.restore, restoreGuide(body ?? {}));
    // 백업 상태를 못 읽어도 명령 안내는 그대로 보인다.
    const state = guide.querySelector('[data-role="backup-state"]');
    if (body === null && state) state.textContent = "백업 상태를 읽지 못함";
  });
  await Promise.allSettled([status, semantic, doctor, backup, sound.ready, messenger]);
}
