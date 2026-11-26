// 설정 화면의 담당 모델 고르기(server 57차 · 스펙 R18-4 · 계약 C32 · 검증 V78). 설정 보강 묶음(`setup/extras.ts`)이
// 설정 상태로 그린 보기 줄(`data-role="models"`)을, 고르기 표(`GET API.setupModels`)를 읽으면 이 표로 바꾼다. 못 읽으면
// 보기 줄 그대로다. 공통 부품 · 같은 출처 부르기만 쓰고, 서버가 준 글자는 글자로만 넣는다. 검사는 서버가 한다.
//
// 그리는 자리(`data-role="models"`, 공통 카드):
// - 담당마다 `tr[data-agent=<id>]`: 담당 이름(서버의 `name`) · 고르기 `select[name="model-<id>"]`(서버의 `choices` 차례,
//   맨 끝 [`MODEL_CUSTOM`] "직접 입력") · 직접 입력 칸 `input[name="model-custom-<id>"]`(직접 입력일 때만 보인다) ·
//   기본값 글.
// - 단추 `button[data-action="save-models"]`(저장) · `button[data-action="reset-models"]`(기본값으로 되돌리기 — 여섯 담당을
//   기본 모델로 바로 저장한다). 저장은 `POST API.setupModels` `{"roles": [{"agent", "model"}]}`(여섯 담당 통째)이고
//   200이면 답대로 다시 그리고 [`MODELS_SAVED`]를 `[data-role="models-result"]`에 보인다. 실패면 오류 줄(`role="alert"`,
//   [`MODELS_ERRORS`])을 보이고 고른 값은 그대로 둔다. 기다리는 동안 단추는 잠긴다.
// - 안내: [`MODELS_COST_NOTE`] · [`MODELS_APPLY_NOTE`]. 다른 기기(`editable` 거짓)면 고르기 · 칸이 잠기고 단추가 없으며
//   [`MODELS_REMOTE_NOTE`]가 보인다.
import { API } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { button, card, errorLine, table } from "../ui/parts";

/** "직접 입력" 고르기의 값. */
export const MODEL_CUSTOM = "__custom__";

/** "직접 입력" 고르기의 글. */
export const MODEL_CUSTOM_LABEL = "직접 입력";

/** 비용 안내. */
export const MODELS_COST_NOTE = "비싼 모델은 하루 AI 호출 상한과 비용에 영향을 줍니다.";

/** 언제 쓰이나 안내. */
export const MODELS_APPLY_NOTE = "바꾼 모델은 다시 켜지 않아도 다음 AI 턴부터 쓰입니다. 도는 논의는 시작할 때의 모델로 끝납니다.";

/** 다른 기기 안내. */
export const MODELS_REMOTE_NOTE = "담당 모델은 이 컴퓨터에서만 바꿀 수 있습니다.";

/** 저장 안내. */
export const MODELS_SAVED = "저장했습니다. 다음 AI 턴부터 쓰입니다.";

/** 바꾸기 실패 까닭 → 화면 글. */
export const MODELS_ERRORS: Readonly<Record<string, string>> = {
  invalid_input: "모델 이름은 영문 소문자 · 숫자 · '-' · '.'로 쓰고, 첫 글자는 영문이나 숫자, 64자까지입니다.",
  local_only: MODELS_REMOTE_NOTE,
};

/** 담당 한 줄. */
export interface ModelRoleRow {
  agent: string;
  name: string;
  model: string;
  default: string;
}

/** 고를 수 있는 모델 하나. */
export interface ModelChoice {
  id: string;
  label: string;
  note: string;
}

/** `GET /api/setup/models`의 답. */
export interface ModelsReply {
  roles: ModelRoleRow[];
  choices: ModelChoice[];
  editable: boolean;
}

/** 객체면 그대로, 아니면 빈 객체. */
function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" ? (value as Record<string, unknown>) : {};
}

/** 칸 넷이 모두 글자인 줄들만. */
function strings<K extends string>(list: unknown, keys: readonly K[]): Record<K, string>[] {
  if (!Array.isArray(list)) return [];
  return list.map(record).filter((row) => keys.every((key) => typeof row[key] === "string")) as Record<K, string>[];
}

/** 답이 고르기 표 모양이면 그 값. */
export function modelsReplyOf(value: unknown): ModelsReply | null {
  const body = record(value);
  if (!Array.isArray(body.roles) || typeof body.editable !== "boolean") return null;
  const roles = strings(body.roles, ["agent", "name", "model", "default"] as const);
  if (roles.length === 0) return null;
  return { roles, choices: strings(body.choices, ["id", "label", "note"] as const), editable: body.editable };
}

/** 바꾸기 실패 까닭 → 화면 글. */
function errorText(status: number, body: unknown): string {
  const reason = record(body).error;
  if (status === 403) return MODELS_REMOTE_NOTE;
  if (typeof reason === "string" && MODELS_ERRORS[reason]) return MODELS_ERRORS[reason];
  return LOAD_FAILED;
}

/** 한 담당의 입력 칸 둘. */
interface RowInputs {
  agent: string;
  pick: HTMLSelectElement;
  typed: HTMLInputElement;
}

/** 고르기 · 직접 입력 칸을 그 모델로 맞춘다(목록에 없으면 직접 입력). */
function show(inputs: RowInputs, model: string, choices: readonly ModelChoice[]): void {
  const listed = choices.some((choice) => choice.id === model);
  inputs.pick.value = listed ? model : MODEL_CUSTOM;
  inputs.typed.value = listed ? "" : model;
  inputs.typed.hidden = listed;
}

/** 한 담당의 고르기 · 직접 입력 칸을 만든다. */
function rowInputs(row: ModelRoleRow, choices: readonly ModelChoice[], editable: boolean): RowInputs {
  const pick = el("select", "ui-input");
  pick.name = `model-${row.agent}`;
  pick.setAttribute("aria-label", `${row.name} 모델`);
  for (const choice of choices) {
    const option = el("option", "", `${choice.label} — ${choice.note}`);
    option.value = choice.id;
    pick.append(option);
  }
  const custom = el("option", "", MODEL_CUSTOM_LABEL);
  custom.value = MODEL_CUSTOM;
  pick.append(custom);
  const typed = el("input", "ui-input");
  typed.name = `model-custom-${row.agent}`;
  typed.setAttribute("aria-label", `${row.name} 모델 직접 입력`);
  pick.disabled = !editable;
  typed.disabled = !editable;
  const inputs = { agent: row.agent, pick, typed };
  show(inputs, row.model, choices);
  pick.addEventListener("change", () => {
    typed.hidden = pick.value !== MODEL_CUSTOM;
    if (!typed.hidden) typed.focus();
  });
  return inputs;
}

/** 고른 값(직접 입력이면 앞뒤 공백을 뗀 칸 값). */
function picked(inputs: RowInputs): string {
  return inputs.pick.value === MODEL_CUSTOM ? inputs.typed.value.trim() : inputs.pick.value;
}

/** 담당 줄 표와 줄마다의 입력 칸. */
function rowsTable(reply: ModelsReply): { grid: HTMLTableElement; inputs: RowInputs[] } {
  const grid = table(["담당", "모델", "직접 입력", "기본값"], reply.roles.map((row) => [row.name, "", "", row.default]));
  const lines = grid.tBodies[0]?.rows ?? [];
  const inputs = reply.roles.map((row, index) => {
    const made = rowInputs(row, reply.choices, reply.editable);
    const tr = lines[index];
    if (tr) {
      tr.dataset.agent = row.agent;
      tr.cells[1]?.append(made.pick);
      tr.cells[2]?.append(made.typed);
    }
    return made;
  });
  return { grid, inputs };
}

/** 고르기 표를 그린다(`data-role="models"`). 저장 · 되돌리기는 이 자리 안에서 다시 그린다. */
export function modelsTable(initial: ModelsReply): HTMLElement {
  let current = initial;
  const wrap = el("div", "setup-extra-wrap");
  const result = el("div", "setup-extra-message");
  result.dataset.role = "models-result";
  result.setAttribute("aria-live", "polite");
  let inputs: RowInputs[] = [];
  const paint = (): void => {
    const made = rowsTable(current);
    inputs = made.inputs;
    wrap.replaceChildren(made.grid);
  };
  const notes = [el("p", "hint", MODELS_COST_NOTE), el("p", "hint", MODELS_APPLY_NOTE)];
  const node = card("담당 모델", wrap, ...notes);
  node.dataset.role = "models";
  node.classList.add("setup-extra");
  paint();
  if (!current.editable) {
    node.append(el("p", "hint", MODELS_REMOTE_NOTE));
    return node;
  }
  const save = button("저장", "accent");
  save.dataset.action = "save-models";
  const reset = button("기본값으로 되돌리기");
  reset.dataset.action = "reset-models";
  const send = (roles: { agent: string; model: string }[]): void => {
    save.disabled = true;
    reset.disabled = true;
    result.replaceChildren();
    void call("POST", API.setupModels, { roles })
      .then((reply) => {
        const next = reply.ok ? modelsReplyOf(reply.body) : null;
        if (!next) return result.replaceChildren(errorLine(errorText(reply.status, reply.body)));
        current = next;
        paint();
        result.replaceChildren(el("p", "hint", MODELS_SAVED));
      })
      .catch(() => result.replaceChildren(errorLine(LOAD_FAILED)))
      .finally(() => {
        save.disabled = false;
        reset.disabled = false;
      });
  };
  save.addEventListener("click", () => send(inputs.map((row) => ({ agent: row.agent, model: picked(row) }))));
  reset.addEventListener("click", () => send(current.roles.map((row) => ({ agent: row.agent, model: row.default }))));
  const actions = el("div", "setup-extra-actions");
  actions.append(save, reset);
  node.append(actions, result);
  return node;
}

/**
 * 고르기 표를 읽어 `root` 안의 보기 줄(`[data-role="models"]`)을 표로 바꾼다. 읽지 못하거나 모양이 다르면 보기 줄
 * 그대로다. 던지지 않는다.
 */
export async function upgradeModels(root: HTMLElement): Promise<void> {
  try {
    const reply = await call("GET", API.setupModels);
    const models = reply.ok ? modelsReplyOf(reply.body) : null;
    const line = root.querySelector('[data-role="models"]');
    if (models && line) line.replaceWith(modelsTable(models));
  } catch {
    // 보기 줄 그대로 둔다.
  }
}
