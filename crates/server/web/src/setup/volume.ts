// 관심 종목 표의 거래량 칸(V32). 서버가 시세 묶음(`quotes` 이벤트)의 종목마다 준 그날 누적 거래량
// (`volume`, 십진수 글자 · 모르면 칸 없음)을 쉬운 말로 그리기만 한다.

const MAN = 10_000;
const EOK = 100_000_000;

/** 소수 한 자리까지(끝의 .0은 뺀다). */
function oneDecimal(value: number): string {
  return (Math.round(value * 10) / 10).toString();
}

/** 거래량 글자를 쉬운 말로: "1,234주" · "1.2만주" · "3.5억주". 십진수 글자가 아니거나 음수면 빈 글자. */
export function volumeText(value: unknown): string {
  if (typeof value !== "string" || !/^[0-9]+(\.[0-9]+)?$/.test(value)) return "";
  const shares = Number(value);
  if (!Number.isFinite(shares)) return "";
  const rounded = Math.round(shares);
  if (rounded < MAN) return `${rounded.toLocaleString("ko-KR")}주`;
  if (Math.round((shares / MAN) * 10) / 10 < MAN) return `${oneDecimal(shares / MAN)}만주`;
  return `${oneDecimal(shares / EOK)}억주`;
}

/** 시세 한 칸의 거래량을 그 칸에 그린다(모르면 비운다). */
export function fillVolume(cell: HTMLElement, quote: unknown): void {
  const volume = typeof quote === "object" && quote !== null ? (quote as Record<string, unknown>).volume : undefined;
  cell.textContent = volumeText(volume);
}
