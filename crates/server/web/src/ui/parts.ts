// 공통 부품(server 26차 2번): 카드 · 표 · 단추(기본 · 강조 · 위험) · 배지(상태) · 입력 칸 · 빈 상태 · 불러오는 중 · 오류 줄.
// 순수 DOM 함수(프레임워크 없음). 글은 모두 글자(`textContent`)로 넣는다 — HTML로 해석하지 않는다.

/** 단추 종류: 기본 · 강조 · 위험. 만든 단추의 `data-kind`에 그대로 적힌다. */
export type ButtonKind = "default" | "accent" | "danger";

/** 배지 상태: 보통 · 성공 · 경고 · 위험. 만든 배지의 `data-tone`에 그대로 적힌다. */
export type BadgeTone = "neutral" | "success" | "warning" | "danger";

/** 단추 `<button type="button" data-kind=…>`(종류를 안 주면 `default`). */
export function button(label: string, kind?: ButtonKind): HTMLButtonElement {
  void label;
  void kind;
  throw new Error("todo: server 26차 단추");
}

/** 상태 배지 `<span data-tone=…>`(상태를 안 주면 `neutral`). */
export function badge(text: string, tone?: BadgeTone): HTMLElement {
  void text;
  void tone;
  throw new Error("todo: server 26차 배지");
}

/** 카드 `<section>`: 첫 자식이 제목(`h2`)이고 그 뒤에 `body`가 차례로 붙는다. */
export function card(title: string, ...body: Node[]): HTMLElement {
  void title;
  void body;
  throw new Error("todo: server 26차 카드");
}

/** 표: 머리 칸은 `thead th[scope=col]`, 줄은 `tbody tr td`. */
export function table(columns: readonly string[], rows: readonly (readonly string[])[]): HTMLTableElement {
  void columns;
  void rows;
  throw new Error("todo: server 26차 표");
}

/** 입력 칸: `label[for=id]`과 그 `input#id`(종류를 안 주면 `text`)를 담은 묶음. */
export function field(label: string, id: string, type?: string): HTMLElement {
  void label;
  void id;
  void type;
  throw new Error("todo: server 26차 입력 칸");
}

/** 빈 상태 줄(`role="status"`, 예: "준비 중"). */
export function emptyState(text: string): HTMLElement {
  void text;
  throw new Error("todo: server 26차 빈 상태");
}

/** 불러오는 중 줄(`role="status"`, `aria-busy="true"`). */
export function loading(text?: string): HTMLElement {
  void text;
  throw new Error("todo: server 26차 불러오는 중");
}

/** 오류 줄(`role="alert"`). */
export function errorLine(text: string): HTMLElement {
  void text;
  throw new Error("todo: server 26차 오류 줄");
}
