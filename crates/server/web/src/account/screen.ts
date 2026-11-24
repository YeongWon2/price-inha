// 계좌 · 운용 화면(server 30차 2번 · 3번 · C31 · V85 · V86 · R3-1 · R3-3 · R14-1 · R16-5 · F1-6 · E57): 팀 상태 카드 ·
// 다시 시작(전체 정지일 때만) · 계좌 카드 · 보유 표 · 늘 보이는 모의계좌 안내. 3D 라이브러리에 닿지 않는다(N21).
// 서버는 `paths.ts`의 경로로만 부른다.
//
// 화면 계약(시험이 고르는 표시, 카드 · 표는 view.ts):
// - 띄우면 세션을 얻고(`POST /api/session`) 팀 상태(`GET /api/team`)와 계좌(`GET /api/account`)를 읽어 그린다.
// - 모의계좌 안내(`data-role="paper-notice"`, `PAPER_NOTICE`)는 읽기 전 · 읽은 뒤 · 읽기 실패 · 어느 팀 상태에서도 보인다.
// - 전체 정지일 때만 `button[data-action="restart"]`가 있다. 누르면 `window.confirm(RESTART_CONFIRM)`을 묻고, 예일 때만
//   `POST /api/team/restart`(몸통 없음)를 보낸다. 결과(`restartText`)는 `data-role="restart-result"`(`role="status"`)에
//   보이고, 그 뒤 팀 상태와 계좌를 다시 읽는다. 아니오면 아무것도 보내지 않는다.
// - 읽기(팀 상태 · 계좌 가운데 하나라도)에 실패하면 `role="alert"` 오류 줄(LOAD_FAILED)과 `button[data-action="retry"]`,
//   다시 시도하면 둘을 다시 읽는다.
// - 실시간: 같은 출처의 `/ws`(`ws:` · `wss:`)를 열고 `REFRESH_KINDS`의 이벤트를 받으면 둘을 다시 읽는다. 이와 별개로
//   `REFRESH_MS`마다 둘을 다시 읽는다.
// - 가볍게: 화면이 숨겨져 있으면(`document.hidden`) 간격 다시 읽기를 쉬고, 다시 보이면 곧바로 한 번 읽는다.
//   읽는 중에 다시 읽기가 오면 겹쳐 부르지 않고 끝난 뒤 한 번만 더 읽는다. 실시간 연결이 끊기면 `REFRESH_MS` 뒤 다시 잇는다.
import "../ui/admin.css";
import "./account.css";
import { API } from "../paths";
import { LOAD_FAILED, call, el } from "../ui/call";
import { button, errorLine, loading } from "../ui/parts";
import {
  REFRESH_KINDS,
  REFRESH_MS,
  RESTART_CONFIRM,
  restartResult,
  restartText,
  type AccountReply,
  type TeamReply,
} from "./model";
import { renderAccount, renderHoldings, renderNotice, renderTeam } from "./view";

/** 몸통이 JSON 객체인가. */
function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** 화면 하나의 상태와 그리기. */
class AccountScreen {
  private readonly body = el("div", "acc-body");
  /** 다시 시작 결과 줄: 다시 그려도 남는다(다시 시작 뒤 단추가 사라져도 결과는 보인다). */
  private readonly result = el("p", "acc-restart-result");
  private running: Promise<void> | null = null;
  private again = false;
  private restarting = false;

  constructor(main: HTMLElement) {
    this.result.dataset.role = "restart-result";
    this.result.setAttribute("role", "status");
    main.append(renderNotice(), this.body);
  }

  renderLoading(): void {
    this.body.replaceChildren(loading());
  }

  /** 팀 상태와 계좌를 다시 읽어 그린다. 읽는 중이면 끝난 뒤 한 번만 더 읽는다. */
  refresh(): Promise<void> {
    if (this.running) {
      this.again = true;
      return this.running;
    }
    this.running = (async () => {
      do {
        this.again = false;
        await this.load();
      } while (this.again);
      this.running = null;
    })();
    return this.running;
  }

  /** 둘을 함께 읽어 그린다. 하나라도 실패하면 오류 줄과 다시 시도. */
  private async load(): Promise<void> {
    try {
      const [team, account] = await Promise.all([call("GET", API.team), call("GET", API.account)]);
      if (!team.ok || !isObject(team.body)) throw new Error(`team ${team.status}`);
      if (!account.ok || !isObject(account.body)) throw new Error(`account ${account.status}`);
      this.render(team.body as unknown as TeamReply, account.body as unknown as AccountReply);
    } catch {
      this.renderFailed();
    }
  }

  private render(team: TeamReply, account: AccountReply): void {
    const teamCard = renderTeam(team, () => void this.restart());
    teamCard.append(this.result);
    const top = el("div", "acc-top");
    top.append(teamCard, renderAccount(account));
    this.body.replaceChildren(top, renderHoldings(account));
  }

  private renderFailed(): void {
    const retry = button("다시 시도", "accent");
    retry.dataset.action = "retry";
    retry.addEventListener("click", () => {
      this.renderLoading();
      void this.refresh();
    });
    const box = el("div", "acc-failed");
    box.append(errorLine(LOAD_FAILED), retry);
    this.body.replaceChildren(box);
  }

  /** 확인 한 번 뒤에만 다시 시작을 보낸다(R3-3 · R14-1: 사람이 화면에서 누르고 확인하는 것이 승인이다). */
  private async restart(): Promise<void> {
    if (this.restarting || !window.confirm(RESTART_CONFIRM)) return;
    this.restarting = true;
    for (const node of this.body.querySelectorAll<HTMLButtonElement>('button[data-action="restart"]')) node.disabled = true;
    try {
      let status = 0;
      let body: unknown = null;
      try {
        ({ status, body } = await call("POST", API.teamRestart));
      } catch {
        // 보내지 못했다: 거절("서버가 받지 않았습니다")로 보인다.
      }
      this.result.textContent = restartText(restartResult(status, body));
      await this.refresh();
    } finally {
      this.restarting = false;
      for (const node of this.body.querySelectorAll<HTMLButtonElement>('button[data-action="restart"]')) node.disabled = false;
    }
  }
}

/** 같은 출처의 `/ws`를 열어 `REFRESH_KINDS` 이벤트에 다시 읽는다. 끊기면 `REFRESH_MS` 뒤 다시 잇는다. */
function listen(screen: AccountScreen): void {
  if (typeof WebSocket === "undefined") return;
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  let socket: WebSocket;
  try {
    socket = new WebSocket(`${scheme}//${location.host}${API.socket}`);
  } catch {
    return;
  }
  socket.onmessage = (event: MessageEvent) => {
    let kind: unknown = null;
    try {
      kind = (JSON.parse(String(event.data)) as { kind?: unknown }).kind;
    } catch {
      return;
    }
    if (typeof kind === "string" && REFRESH_KINDS.includes(kind)) void screen.refresh();
  };
  socket.onclose = () => {
    setTimeout(() => listen(screen), REFRESH_MS);
  };
}

/**
 * 본문 자리(`main`)에 계좌 · 운용 화면을 그린다: 세션을 얻고 팀 상태 · 계좌를 읽어 그린다.
 * 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
 */
export async function mountAccount(main: HTMLElement | null): Promise<void> {
  if (!main) return;
  const screen = new AccountScreen(main);
  screen.renderLoading();
  try {
    await call("POST", API.session);
  } catch {
    // 세션을 얻지 못해도 읽기를 해 본다: 실패하면 오류 줄과 다시 시도가 보인다.
  }
  await screen.refresh();
  listen(screen);
  setInterval(() => {
    if (!document.hidden) void screen.refresh();
  }, REFRESH_MS);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) void screen.refresh();
  });
}
