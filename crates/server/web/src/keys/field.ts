// 키 칸 규칙(설정 · 키 화면이 함께 쓴다): 가린 칸(password) · 브라우저가 저장을 권하지 않음(new-password) ·
// 맞춤법 보지 않음(spellcheck=false) · 등록에 성공하면 칸을 비움 · 서버 답의 값으로 칸을 다시 채우지 않음.

/** 키 값을 보내는 쪽. 받아들여졌으면 `true`. */
export type KeySend = (values: Record<string, string>) => Promise<boolean>;

/** 칸에 키 칸 규칙을 건다. */
export function applyKeyFieldRules(input: HTMLInputElement): void {
  input.type = "password";
  input.setAttribute("autocomplete", "new-password");
  input.setAttribute("spellcheck", "false");
}

/** 규칙을 건 새 키 칸. */
export function keyInput(id: string): HTMLInputElement {
  const input = document.createElement("input");
  input.id = id;
  applyKeyFieldRules(input);
  return input;
}

/** 칸들을 비운다(값을 다시 채우는 곳은 없다). */
export function clearKeyFields(fields: Iterable<HTMLInputElement>): void {
  for (const input of fields) input.value = "";
}

/**
 * 칸들의 값을 `send`로 한 번 보낸다. 받아들여지면 모든 칸을 비우고 `true`,
 * 거절되거나 실패하면(던져도) 칸을 그대로 두고 `false`.
 */
export async function submitKeyFields(
  fields: Record<string, HTMLInputElement>,
  send: KeySend,
): Promise<boolean> {
  const values: Record<string, string> = {};
  for (const [name, input] of Object.entries(fields)) values[name] = input.value;
  let accepted = false;
  try {
    accepted = (await send(values)) === true;
  } catch {
    accepted = false;
  }
  if (accepted) clearKeyFields(Object.values(fields));
  return accepted;
}
