// src/keys/field.ts
/** 칸에 키 칸 규칙을 건다. */
function applyKeyFieldRules(input) {
	input.type = "password";
	input.setAttribute("autocomplete", "new-password");
	input.setAttribute("spellcheck", "false");
}
/** 칸들을 비운다(값을 다시 채우는 곳은 없다). */
function clearKeyFields(fields) {
	for (const input of fields) input.value = "";
}
/**
* 칸들의 값을 `send`로 한 번 보낸다. 받아들여지면 모든 칸을 비우고 `true`,
* 거절되거나 실패하면(던져도) 칸을 그대로 두고 `false`.
*/
async function submitKeyFields(fields, send) {
	const values = {};
	for (const [name, input] of Object.entries(fields)) values[name] = input.value;
	let accepted = false;
	try {
		accepted = await send(values) === true;
	} catch {
		accepted = false;
	}
	if (accepted) clearKeyFields(Object.values(fields));
	return accepted;
}
export { submitKeyFields as n, applyKeyFieldRules as t };
