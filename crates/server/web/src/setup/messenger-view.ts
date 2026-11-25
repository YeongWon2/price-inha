// 설정 화면의 "메신저" 묶음(server 39차 7번, 결정 24). 설정 보강(`extras.ts`의 `mountSetupExtras`)이 자리 하나
// (`data-role="messenger"`)에 [`mountMessenger`]로 단다. 공통 부품(`ui/parts.ts`) · 같은 출처 부르기(`ui/call.ts`)만 쓰고
// 서버가 준 글자는 글자로만 넣는다(브라우저 저장소 · 쿠키에 남기지 않는다).
//
// 그리는 자리(`data-role`, 모두 `[data-role="messenger"]` 안):
// - `messenger-state`: [`STATE_LABELS`]의 글, `data-state`에 상태 글자.
// - `messenger-bot`: 봇 이름이 있으면 `@봇이름`.
// - 이 컴퓨터(`editable`)면: 토큰 칸 `input#messenger-token`(`type=password` · `autocomplete=new-password`) ·
//   등록 `button[data-action="messenger-register"]`(→ `POST API.messengerTelegram` `{"token"}`), 토큰이 있으면 지우기
//   `button[data-action="messenger-remove"]`(→ `DELETE API.messengerTelegram`). 보낸 뒤에는 토큰 칸을 비운다.
//   - 토큰이 있고 연결된 사람이 없으면 `messenger-pairing`: [`PAIRING_HINT`] · 코드 칸 `input#messenger-code` ·
//     승인 `button[data-action="messenger-approve"]`(→ `POST API.messengerApprove` `{"code"}`).
//   - 연결된 사람이 있으면 `messenger-paired`: [`PAIRED_TEXT`] · 빼기 `button[data-action="messenger-revoke"]`
//     (→ `POST API.messengerRevoke`, 몸통 없음).
// - 다른 기기(`editable` 거짓)면: 입력 · 단추 없이 `messenger-remote`([`REMOTE_NOTE`]). 연결됐으면 [`PAIRED_TEXT`]는 보인다.
// - `messenger-slack`: [`SLACK_SOON`] 한 줄(늘).
// - `messenger-message`: 결과 자리. 실패면 오류 줄(`role="alert"`, [`messengerError`]의 글). 성공이면 상태를 다시
//   읽어(`GET API.messenger`) 묶음을 새로 그린다. 기다리는 동안 그 단추는 잠긴다.
// 읽기 실패: 이 묶음에 오류 줄(`LOAD_FAILED`)만 보이고 던지지 않는다.
import { API } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { badge, button, card, errorLine, field, loading } from "../ui/parts";
import type { BadgeTone } from "../ui/parts";
import {
  PAIRED_TEXT,
  PAIRING_HINT,
  REMOTE_NOTE,
  SLACK_SOON,
  STATE_LABELS,
  approveBody,
  messengerError,
  parseMessengerStatus,
  registerBody,
} from "./messenger-model";
import type { MessengerState, MessengerStatus } from "./messenger-model";

/** 묶음 제목. */
const TITLE = "메신저";

/** 상태 배지의 빛깔(공통 부품의 상태 이름만 쓴다). */
const STATE_TONES: Readonly<Record<MessengerState, BadgeTone>> = {
  off: "neutral",
  polling: "success",
  unauthorized: "danger",
  error: "warning",
};

/** 요청 하나: 방법 · 경로 · 몸통(없으면 몸통 없음). */
interface Ask {
  method: "POST" | "DELETE";
  url: string;
  body?: unknown;
}

/** 이 묶음의 겉(`data-role="messenger"`). */
function shell(...body: Node[]): HTMLElement {
  const node = card(TITLE, ...body);
  node.dataset.role = "messenger";
  return node;
}

/** 상태를 읽는다. 실패(상태 · 연결 · 모양)면 `null`. */
async function readStatus(): Promise<MessengerStatus | null> {
  try {
    const reply = await call("GET", API.messenger);
    return reply.ok ? parseMessengerStatus(reply.body) : null;
  } catch {
    return null;
  }
}

/** 읽지 못했을 때의 묶음. */
function failedBlock(): HTMLElement {
  const message = el("div", "");
  message.dataset.role = "messenger-message";
  message.append(errorLine(LOAD_FAILED));
  return shell(message);
}

/** 단추 하나를 만든다(`data-action`). */
function actionButton(label: string, action: string, kind: "default" | "accent" | "danger" = "default"): HTMLButtonElement {
  const node = button(label, kind);
  node.dataset.action = action;
  return node;
}

/** 단추를 누르면 `ask()`가 만든 요청을 보내고, 됐으면 다시 읽어 묶음을 새로 그린다. 실패면 결과 자리에 오류 줄. */
function wire(box: HTMLElement, message: HTMLElement, node: HTMLButtonElement, ask: () => Ask): void {
  node.addEventListener("click", () => {
    node.disabled = true;
    message.replaceChildren();
    void (async () => {
      const { method, url, body } = ask();
      let text: string | null = null;
      try {
        const reply = await call(method, url, body);
        if (!reply.ok) text = messengerError(reply.status, reply.body);
      } catch {
        text = LOAD_FAILED;
      }
      if (text === null) {
        const next = await readStatus();
        if (next !== null) {
          box.replaceWith(messengerBlock(next));
          return;
        }
        text = LOAD_FAILED;
      }
      message.replaceChildren(errorLine(text));
      node.disabled = false;
    })();
  });
}

/** 칸 하나의 입력 요소(`field`가 만든 것). */
function inputOf(holder: HTMLElement): HTMLInputElement {
  return holder.querySelector("input") as HTMLInputElement;
}

/** 상태 줄: 배지(`messenger-state`)와 봇 이름(`messenger-bot`, 있으면). */
function stateLine(status: MessengerStatus): HTMLElement {
  const line = el("div", "");
  const state = status.telegram.state;
  const mark = badge(STATE_LABELS[state], STATE_TONES[state]);
  mark.dataset.role = "messenger-state";
  mark.dataset.state = state;
  line.append(el("span", "", "텔레그램 "), mark);
  if (status.telegram.bot !== null) {
    const bot = el("span", "", ` @${status.telegram.bot}`);
    bot.dataset.role = "messenger-bot";
    line.append(bot);
  }
  return line;
}

/** 이 컴퓨터에서 바꾸는 칸 · 단추들. */
function editParts(box: HTMLElement, message: HTMLElement, status: MessengerStatus): Node[] {
  const parts: Node[] = [];
  const tokenField = field("봇 토큰", "messenger-token", "password");
  const token = inputOf(tokenField);
  token.autocomplete = "new-password";
  const register = actionButton("등록", "messenger-register", "accent");
  wire(box, message, register, () => {
    const body = registerBody(token.value);
    token.value = "";
    return { method: "POST", url: API.messengerTelegram, body };
  });
  parts.push(tokenField, register);
  const telegram = status.telegram;
  if (telegram.configured) {
    const remove = actionButton("지우기", "messenger-remove", "danger");
    wire(box, message, remove, () => ({ method: "DELETE", url: API.messengerTelegram }));
    parts.push(remove);
  }
  if (telegram.configured && !telegram.paired) {
    const pairing = el("div", "");
    pairing.dataset.role = "messenger-pairing";
    const codeField = field("페어링 코드", "messenger-code");
    const code = inputOf(codeField);
    code.autocomplete = "off";
    const approve = actionButton("승인", "messenger-approve", "accent");
    wire(box, message, approve, () => ({ method: "POST", url: API.messengerApprove, body: approveBody(code.value) }));
    pairing.append(el("p", "", PAIRING_HINT), codeField, approve);
    parts.push(pairing);
  }
  if (telegram.paired) {
    const paired = el("div", "");
    paired.dataset.role = "messenger-paired";
    const revoke = actionButton("빼기", "messenger-revoke", "danger");
    wire(box, message, revoke, () => ({ method: "POST", url: API.messengerRevoke }));
    paired.append(el("span", "", `${PAIRED_TEXT} `), revoke);
    parts.push(paired);
  }
  return parts;
}

/** 다른 기기에서 보는 줄들(입력 · 단추 없음). */
function remoteParts(status: MessengerStatus): Node[] {
  const note = el("p", "", REMOTE_NOTE);
  note.dataset.role = "messenger-remote";
  const parts: Node[] = [note];
  if (status.telegram.paired) {
    const paired = el("p", "", PAIRED_TEXT);
    paired.dataset.role = "messenger-paired";
    parts.push(paired);
  }
  return parts;
}

/** 상태 하나로 메신저 묶음(`data-role="messenger"`)을 그린다. 단추는 위 계약대로 서버를 부르고 다시 그린다. */
export function messengerBlock(status: MessengerStatus): HTMLElement {
  const message = el("div", "");
  message.dataset.role = "messenger-message";
  const slack = el("p", "", SLACK_SOON);
  slack.dataset.role = "messenger-slack";
  const box = shell();
  const middle = status.editable ? editParts(box, message, status) : remoteParts(status);
  box.append(stateLine(status), ...middle, slack, message);
  return box;
}

/** 자리 하나에 메신저 묶음을 읽어 그린다(`GET API.messenger`). 읽기가 실패해도 던지지 않는다. */
export async function mountMessenger(root: HTMLElement): Promise<void> {
  root.replaceChildren(shell(loading()));
  const status = await readStatus();
  root.replaceChildren(status === null ? failedBlock() : messengerBlock(status));
}
