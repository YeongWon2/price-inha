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
export { clearKeyFields as n, applyKeyFieldRules as t };
