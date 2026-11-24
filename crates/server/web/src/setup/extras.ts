// 설정 화면 보강(server 31차 1번 · 스펙 R18-4 · R16-4 · 계약 C32 · 검증 V78): 시장 · 담당 모델 줄, 뜻 검색 카드,
// 진단 표, 되살리기 안내, 알림음 설정. 설정 화면(`setup/main.ts`)이 [`mountSetupExtras`]로 자리 하나에 단다.
// 공통 부품(`ui/parts.ts`) · 같은 출처 부르기(`ui/call.ts`)만 쓰고 3D에 닿지 않는다. 서버가 준 글자는 글자로만 넣는다.
//
// 읽는 경로(모두 `GET`, 경로 표 `SCREEN_PATHS.setup` 안): 설정 상태 `API.setup`(`market` · `models` 칸),
// 뜻 검색 `API.setupSemantic`, 진단 `API.setupDoctor`, 백업 `API.setupBackup`. 바꾸는 경로는 뜻 검색 `POST` 하나뿐이다
// (시장 · 모델은 보기만, 되살리기는 화면에서 하지 않는다 — 실행 중인 저장소를 바꾸지 않는다).
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
// - `sound`: 체크 칸 `#sound-on`(이름 [`SOUND_LABEL`]) — `ui/sound-pref.ts`로 읽고 남긴다(기본 켜짐).
//
// 읽기 실패: 그 경로를 쓰는 자리만 오류 줄(`role="alert"`, `LOAD_FAILED`)을 보이고 다른 자리는 그대로 그린다.
import type { BadgeTone } from "../ui/parts";

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
export const SOUND_LABEL = "승인 알림음 (이 기기에만 남습니다)";

/** 설정 상태 몸통으로 시장 줄을 그린다(`data-role="market"`). */
export function marketLine(status: unknown): HTMLElement {
  throw new Error("todo");
}

/** 설정 상태 몸통으로 담당 모델 줄을 그린다(`data-role="models"`). */
export function modelsLine(status: unknown): HTMLElement {
  throw new Error("todo");
}

/** 진단 답으로 진단 표를 그린다(`data-role="diagnosis"`). */
export function diagnosisTable(reply: unknown): HTMLElement {
  throw new Error("todo");
}

/** 백업 설정 답(`{"repo", "recipient", "configured"}`)으로 되살리기 안내를 그린다(`data-role="restore"`). */
export function restoreGuide(backup: unknown): HTMLElement {
  throw new Error("todo");
}

/** 뜻 검색 카드를 그린다(`data-role="semantic"`). 단추를 누르면 바꾸기를 보낸다. */
export function semanticCard(initial: SemanticSettings): HTMLElement {
  throw new Error("todo");
}

/** 알림음 설정을 그린다(`data-role="sound"`). */
export function soundSetting(): HTMLElement {
  throw new Error("todo");
}

/** 자리 하나에 보강 묶음 모두를 읽어 그린다. 읽기가 실패해도 던지지 않는다. */
export async function mountSetupExtras(root: HTMLElement): Promise<void> {
  throw new Error("todo");
}
