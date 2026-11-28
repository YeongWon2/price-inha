// 관심 종목 표의 거래량 칸(V32). 서버가 시세 묶음(`quotes` 이벤트)의 종목마다 준 그날 누적 거래량
// (`volume`, 십진수 글자 · 모르면 칸 없음)을 쉬운 말로 그리기만 한다.

/** 거래량 글자를 쉬운 말로: 아직 뼈대다. */
export function volumeText(value: unknown): string {
  void value;
  return "";
}

/** 시세 한 칸의 거래량을 그 칸에 그린다(모르면 비운다). */
export function fillVolume(cell: HTMLElement, quote: unknown): void {
  void quote;
  cell.textContent = "";
}
