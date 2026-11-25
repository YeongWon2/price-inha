// 규칙 화면의 금지 종목 · 업종표 묶음(server 35차 3번, 결정 19번 · P5 · P8 · R7-3 · F5)의 화면 모델: 서버 응답을
// 그릴 값으로 바꾸고, 고치는 중인 값(초안)에 줄을 더하고 지우고, 저장 요청 몸통과 결과 문구를 만드는 순수 함수.
// DOM을 만지지 않는다(그리기는 config-view.ts, 서버 부르기는 screen.ts).
//
// 서버 계약(`GET` · `POST /api/rules/config`, Rust `RULES_CONFIG_PATH`):
// - 읽기 `{"banned": [{"market": "kr" | "us", "symbol"}], "sectors": [{"symbol", "sector"}], "editable": bool,
//   "unsectored": [종목 글자]}` — 금지 종목은 저장된 차례, 업종표는 종목 차례, `editable`은 그 컴퓨터에서 고칠 수
//   있는가(원격이면 거짓), `unsectored`는 지켜보는 종목 가운데 업종표에 없는 것.
// - 고치기 `{"banned": [...], "sectors": [{"symbol", "sector"}]}`로 통째 바꾼다. 200이면 읽기와 같은 모양,
//   원격이면 403, 값 검사에 걸리면 400 `{"error": "invalid_input"}`.
// - 값 검사(서버와 같은 규칙): 종목은 앞뒤 공백을 뗀 뒤 영문 · 숫자 · `.` · `-` 1~32자, 시장은 `kr` · `us`,
//   업종 이름은 앞뒤 공백을 뗀 뒤 1자 이상 [`SECTOR_MAX_CHARS`]자 이하(글자 수는 코드 포인트로 센다), 같은
//   (시장, 종목)이 금지 목록에 두 번 · 같은 종목이 업종표에 두 번이면 안 된다.

/** 시장 글자. */
export type MarketWord = "kr" | "us";

/** 금지 종목 한 줄. */
export interface BannedRow {
  market: MarketWord;
  symbol: string;
}

/** 업종표 한 줄. */
export interface SectorRow {
  symbol: string;
  sector: string;
}

/** 읽기 응답(`GET /api/rules/config`). */
export interface RulesConfigReply {
  banned: BannedRow[];
  sectors: SectorRow[];
  editable: boolean;
  unsectored: string[];
}

/** 고치는 중인 값(저장 전 초안). 늘 새 값으로 바꾸고 고치지 않는다. */
export interface ConfigDraft {
  banned: BannedRow[];
  sectors: SectorRow[];
}

/** 업종 이름의 가장 긴 글자 수(서버 `RULES_SECTOR_MAX_CHARS`와 같다). */
export const SECTOR_MAX_CHARS = 40;

/** 이 컴퓨터가 아니면 보이는 안내 글. */
export const READONLY_TEXT = "이 컴퓨터에서만 바꿀 수 있습니다";

/** 줄 더하기 결과: 새 초안 또는 왜 더하지 않았는지(사람이 읽는 글, 비지 않는다). */
export type AddResult = { ok: true; draft: ConfigDraft } | { ok: false; problem: string };

/** 저장 결과 문구: 저장함 · 실패(원격 403은 [`READONLY_TEXT`]가 든다, 400은 값이 올바르지 않다는 글). */
export type SaveMessage = { kind: "saved"; text: string } | { kind: "failed"; text: string };

/**
 * 읽기 응답 몸통을 확인해 값으로 바꾼다. 모양이 맞지 않으면(칸이 없거나 종류가 다름) 없음.
 * 모르는 칸은 버린다.
 */
export function readConfig(body: unknown): RulesConfigReply | null {
  void body;
  throw new Error("todo");
}

/** 종목 글자인가(앞뒤 공백을 뗀 뒤 영문 · 숫자 · `.` · `-` 1~32자). */
export function isSymbol(text: string): boolean {
  void text;
  throw new Error("todo");
}

/** 읽은 값에서 초안을 만든다(줄을 복사한다 — 읽은 값을 바꾸지 않는다). */
export function draftOf(reply: RulesConfigReply): ConfigDraft {
  void reply;
  throw new Error("todo");
}

/**
 * 금지 종목 한 줄을 끝에 더한다(종목은 앞뒤 공백을 뗀다). 시장이 `kr` · `us`가 아니거나 종목 글자가 아니거나
 * 같은 (시장, 종목)이 이미 있으면 더하지 않고 까닭을 준다. 받은 초안은 바꾸지 않는다.
 */
export function addBanned(draft: ConfigDraft, market: string, symbol: string): AddResult {
  void draft;
  void market;
  void symbol;
  throw new Error("todo");
}

/** 그 차례의 금지 종목 줄을 뺀 새 초안(차례 밖이면 같은 줄들). */
export function removeBanned(draft: ConfigDraft, index: number): ConfigDraft {
  void draft;
  void index;
  throw new Error("todo");
}

/**
 * 업종표 한 줄을 끝에 더한다(종목 · 업종 이름은 앞뒤 공백을 뗀다). 종목 글자가 아니거나, 업종 이름이 비었거나
 * [`SECTOR_MAX_CHARS`]자를 넘거나, 같은 종목이 이미 있으면 더하지 않고 까닭을 준다. 받은 초안은 바꾸지 않는다.
 */
export function addSector(draft: ConfigDraft, symbol: string, sector: string): AddResult {
  void draft;
  void symbol;
  void sector;
  throw new Error("todo");
}

/** 그 차례의 업종표 줄을 뺀 새 초안(차례 밖이면 같은 줄들). */
export function removeSector(draft: ConfigDraft, index: number): ConfigDraft {
  void draft;
  void index;
  throw new Error("todo");
}

/** 저장 요청 몸통 `{"banned": [{market, symbol}], "sectors": [{symbol, sector}]}`(초안의 차례 그대로, 다른 칸 없음). */
export function saveBody(draft: ConfigDraft): { banned: BannedRow[]; sectors: SectorRow[] } {
  void draft;
  throw new Error("todo");
}

/**
 * 업종 없는 종목 경고 줄: `unsectored`가 있으면 "업종이 없어 살 수 없는 종목: 005930, 000660"(응답 차례, `", "`로
 * 잇는다), 없으면 없음.
 */
export function unsectoredLine(reply: RulesConfigReply): string | null {
  void reply;
  throw new Error("todo");
}

/**
 * 저장 결과 → 문구. 200이면 저장함("저장했습니다"가 든다, 다음 주문 전 점검부터 쓰인다는 뜻), 403이면 실패이고
 * [`READONLY_TEXT`]가 든다, 400이면 실패이고 "올바르지 않"이 든다, 그 밖(5xx 등 · 연결 실패는 `status` 0)은
 * 실패이고 "다시 시도"가 든다.
 */
export function saveMessage(status: number, body: unknown): SaveMessage {
  void status;
  void body;
  throw new Error("todo");
}
