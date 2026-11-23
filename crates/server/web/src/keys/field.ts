// 키 칸 규칙(설정 · 키 화면이 함께 쓴다): 가린 칸(password) · 브라우저가 저장을 권하지 않음(new-password) ·
// 맞춤법 보지 않음(spellcheck=false) · 등록에 성공하면 칸을 비움 · 서버 답의 값으로 칸을 다시 채우지 않음.
// N1 뼈대: 몸통은 N2가 채운다.

/** 키 값을 보내는 쪽. 받아들여졌으면 `true`. */
export type KeySend = (values: Record<string, string>) => Promise<boolean>;

/** 칸에 키 칸 규칙을 건다. */
export function applyKeyFieldRules(input: HTMLInputElement): void {
  throw new Error(`아직 없음(N2): applyKeyFieldRules(${input.id})`);
}

/** 규칙을 건 새 키 칸. */
export function keyInput(id: string): HTMLInputElement {
  throw new Error(`아직 없음(N2): keyInput(${id})`);
}

/**
 * 칸들의 값을 `send`로 한 번 보낸다. 받아들여지면 모든 칸을 비우고 `true`,
 * 거절되거나 실패하면(던져도) 칸을 그대로 두고 `false`.
 */
export async function submitKeyFields(
  fields: Record<string, HTMLInputElement>,
  send: KeySend,
): Promise<boolean> {
  throw new Error(`아직 없음(N2): submitKeyFields(${Object.keys(fields).length}, ${typeof send})`);
}
