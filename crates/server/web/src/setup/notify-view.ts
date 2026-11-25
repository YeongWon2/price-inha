// 설정 화면의 "알림" 묶음(server 40차 6번, 결정 24). 설정 보강(`extras.ts`의 `mountSetupExtras`)이 자리 하나
// (`data-role="notify"`)에 [`mountNotify`]로 단다(N2가 단다). 공통 부품(`ui/parts.ts`) · 같은 출처 부르기(`ui/call.ts`)만
// 쓰고 서버가 준 글자는 글자로만 넣는다(브라우저 저장소 · 쿠키에 남기지 않는다). 계산 · 검사는 서버가 한다.
//
// 그리는 자리(`data-role`, 모두 `[data-role="notify"]` 안):
// - `notify-kinds`: 표. 종류마다 `tr[data-kind="<종류 글자>"]` 한 줄(서버가 준 차례) — 종류 글(`label`) · 폰 칸
//   `input[type=checkbox][name="push"]` · 메신저 칸 `input[type=checkbox][name="messenger"]`. 잠긴 줄은
//   `data-locked="true"`이고 [`LOCKED_TEXT`]가 보인다(칸은 그대로 켜고 끌 수 있다 — 둘 다 끄면 서버가 400).
// - `notify-quiet`: 조용한 시간 — 켜기 `input#notify-quiet-on`(checkbox) · 시작 `input#notify-quiet-start` · 끝
//   `input#notify-quiet-end`(값은 `HH:MM` 글자) · [`QUIET_NOTE`].
// - 이 컴퓨터(`editable`)면 저장 `button[data-action="notify-save"]` → `POST API.notifySettings`(`saveBody`의 모양,
//   지금 칸 값으로). 기다리는 동안 단추는 잠긴다. 성공이면 받은 몸통으로 묶음을 다시 그리고 `notify-message`에
//   [`SAVED_TEXT`](`role="status"`). 실패(400 · 403 · 그 밖)면 `notify-message`에 오류 줄(`role="alert"`,
//   `notifyError`의 글)이고 칸 값은 그대로 둔다.
// - 다른 기기(`editable` 거짓)면 저장 단추가 없고 모든 칸이 잠기며(`disabled`) `notify-remote`([`REMOTE_NOTE`])가 보인다.
// 읽기 실패: 이 묶음에 오류 줄(`LOAD_FAILED`)만 보이고 던지지 않는다.
import { API } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { badge, button, card, errorLine, field, loading, table } from "../ui/parts";
import {
  LOCKED_NOTE,
  LOCKED_TEXT,
  NOTIFY_TITLE,
  QUIET_NOTE,
  REMOTE_NOTE,
  SAVED_TEXT,
  notifyError,
  parseNotifySettings,
  saveBody,
} from "./notify-model";
import type { NotifyKindRow, NotifySettings } from "./notify-model";

/** 표의 머리 칸. */
const COLUMNS = ["종류", "폰", "메신저"] as const;

/** 이 묶음의 겉(`data-role="notify"`). */
function shell(...body: Node[]): HTMLElement {
  const node = card(NOTIFY_TITLE, ...body);
  node.dataset.role = "notify";
  return node;
}

/** 설정을 읽는다. 실패(상태 · 연결 · 모양)면 `null`. */
async function readSettings(): Promise<NotifySettings | null> {
  try {
    const reply = await call("GET", API.notifySettings);
    return reply.ok ? parseNotifySettings(reply.body) : null;
  } catch {
    return null;
  }
}

/** 켜고 끄는 칸 하나(`input[type=checkbox][name]`). */
function toggle(name: "push" | "messenger", checked: boolean, label: string, editable: boolean): HTMLInputElement {
  const input = el("input", "");
  input.type = "checkbox";
  input.name = name;
  input.checked = checked;
  input.disabled = !editable;
  input.setAttribute("aria-label", label);
  return input;
}

/** 종류 한 줄(`tr[data-kind]`, 잠긴 줄은 `data-locked="true"`와 [`LOCKED_TEXT`]). */
function kindRow(tbody: HTMLTableSectionElement, row: NotifyKindRow, editable: boolean): void {
  const tr = tbody.insertRow();
  tr.dataset.kind = row.kind;
  if (row.locked) tr.dataset.locked = "true";
  const name = tr.insertCell();
  name.append(el("span", "", row.label));
  if (row.locked) name.append(" ", badge(LOCKED_TEXT, "warning"));
  tr.insertCell().append(toggle("push", row.push, `${row.label} 폰 알림`, editable));
  tr.insertCell().append(toggle("messenger", row.messenger, `${row.label} 메신저 알림`, editable));
}

/** 종류별 표(`data-role="notify-kinds"`). */
function kindsTable(settings: NotifySettings): HTMLElement {
  const grid = table(COLUMNS, []);
  const tbody = grid.tBodies[0] ?? grid.createTBody();
  for (const row of settings.kinds) kindRow(tbody, row, settings.editable);
  const holder = el("div", "");
  holder.dataset.role = "notify-kinds";
  holder.append(grid, el("p", "", LOCKED_NOTE));
  return holder;
}

/** 칸 묶음(`field`)의 입력 요소. */
function inputOf(holder: HTMLElement): HTMLInputElement {
  return holder.querySelector("input") as HTMLInputElement;
}

/** 조용한 시간 칸(`data-role="notify-quiet"`): 켜기 · 시작 · 끝(`HH:MM`). */
function quietPart(settings: NotifySettings): HTMLElement {
  const on = field("조용한 시간 켜기", "notify-quiet-on", "checkbox");
  on.classList.add("setup-extra-check");
  const start = field("시작", "notify-quiet-start", "time");
  const end = field("끝", "notify-quiet-end", "time");
  inputOf(on).checked = settings.quiet.enabled;
  inputOf(start).value = settings.quiet.start;
  inputOf(end).value = settings.quiet.end;
  for (const holder of [on, start, end]) inputOf(holder).disabled = !settings.editable;
  const part = el("div", "");
  part.dataset.role = "notify-quiet";
  part.append(on, start, end, el("p", "", QUIET_NOTE));
  return part;
}

/** 지금 칸 값으로 바꾼 설정(서버가 준 차례 그대로). */
function currentValues(box: HTMLElement, settings: NotifySettings): NotifySettings {
  const checked = (selector: string, fallback: boolean): boolean =>
    box.querySelector<HTMLInputElement>(selector)?.checked ?? fallback;
  const value = (selector: string, fallback: string): string =>
    box.querySelector<HTMLInputElement>(selector)?.value ?? fallback;
  const lines = [...box.querySelectorAll<HTMLTableRowElement>("tr[data-kind]")];
  const kinds = settings.kinds.map((row) => {
    const line = lines.find((tr) => tr.dataset.kind === row.kind);
    const flag = (name: string): boolean | undefined =>
      line?.querySelector<HTMLInputElement>(`input[name="${name}"]`)?.checked;
    return { ...row, push: flag("push") ?? row.push, messenger: flag("messenger") ?? row.messenger };
  });
  const quiet = {
    enabled: checked("#notify-quiet-on", settings.quiet.enabled),
    start: value("#notify-quiet-start", settings.quiet.start),
    end: value("#notify-quiet-end", settings.quiet.end),
  };
  return { ...settings, kinds, quiet };
}

/** 저장 단추: 지금 칸 값으로 통째 보낸다. 성공이면 받은 값으로 다시 그리고, 실패면 오류 줄(칸 값은 그대로). */
function saveButton(box: HTMLElement, message: HTMLElement, settings: NotifySettings): HTMLButtonElement {
  const save = button("저장", "accent");
  save.dataset.action = "notify-save";
  save.addEventListener("click", () => {
    save.disabled = true;
    message.replaceChildren();
    void (async () => {
      let text: string;
      try {
        const reply = await call("POST", API.notifySettings, saveBody(currentValues(box, settings)));
        const next = reply.ok ? parseNotifySettings(reply.body) : null;
        if (next !== null) {
          const fresh = notifyBlock(next);
          const done = fresh.querySelector<HTMLElement>('[data-role="notify-message"]');
          if (done) done.replaceChildren(savedLine());
          box.replaceWith(fresh);
          return;
        }
        text = notifyError(reply.status, reply.body, settings.kinds);
      } catch {
        text = notifyError(0, null);
      }
      message.replaceChildren(errorLine(text));
      save.disabled = false;
    })();
  });
  return save;
}

/** 저장 성공 줄(`role="status"`). */
function savedLine(): HTMLElement {
  const line = el("p", "", SAVED_TEXT);
  line.setAttribute("role", "status");
  return line;
}

/** 설정 하나로 알림 묶음(`data-role="notify"`)을 그린다. 단추는 위 계약대로 서버를 부르고 다시 그린다. */
export function notifyBlock(settings: NotifySettings): HTMLElement {
  const message = el("div", "");
  message.dataset.role = "notify-message";
  const box = shell(kindsTable(settings), quietPart(settings));
  if (settings.editable) {
    box.append(saveButton(box, message, settings));
  } else {
    const remote = el("p", "", REMOTE_NOTE);
    remote.dataset.role = "notify-remote";
    box.append(remote);
  }
  box.append(message);
  return box;
}

/** 읽지 못했을 때의 묶음. */
function failedBlock(): HTMLElement {
  const message = el("div", "");
  message.dataset.role = "notify-message";
  message.append(errorLine(LOAD_FAILED));
  return shell(message);
}

/** 자리 하나에 알림 묶음을 읽어 그린다(`GET API.notifySettings`). 읽기가 실패해도 던지지 않는다. */
export async function mountNotify(root: HTMLElement): Promise<void> {
  root.replaceChildren(shell(loading()));
  const settings = await readSettings();
  root.replaceChildren(settings === null ? failedBlock() : notifyBlock(settings));
}
