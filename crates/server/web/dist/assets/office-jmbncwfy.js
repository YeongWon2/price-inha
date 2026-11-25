import { a as approvalPath, n as API, o as decidePath, r as OFFICE_LOOK_PATHS, t as mountMenu } from "./menu-iyrf4dyx.js";
import { n as soundEnabled } from "./sound-pref-d3wzz2ba.js";
var PENDING$1 = "pending";
var CODE_LENGTH = 6;
function parse(payload) {
	if (typeof payload !== "object" || payload === null) return null;
	const p = payload;
	if (typeof p.number !== "string" || p.number === "") return null;
	if (typeof p.summary !== "string" || typeof p.short_code !== "string" || typeof p.status !== "string") return null;
	return {
		number: p.number,
		summary: p.summary,
		short_code: p.short_code,
		status: p.status
	};
}
function findCard(root, number) {
	for (const card of root.querySelectorAll("[data-approval]")) if (card.dataset.approval === number) return card;
	return null;
}
function part(card, name) {
	return card.querySelector(`[data-part="${name}"]`);
}
function textPart(tag, name, className = "") {
	const element = document.createElement(tag);
	element.dataset.part = name;
	if (className) element.className = className;
	return element;
}
function verbButton(root, number, verb, fetcher, resync) {
	const button = document.createElement("button");
	button.type = "button";
	button.dataset.verb = verb;
	button.textContent = verb === "approve" ? "승인" : "거절";
	button.addEventListener("click", () => {
		decideApproval(root, number, verb, fetcher, resync);
	});
	return button;
}
function newCard(root, number, fetcher, resync) {
	const card = document.createElement("article");
	card.className = "approval-card";
	card.dataset.approval = number;
	const title = document.createElement("h2");
	title.textContent = "승인이 필요해요";
	const code = document.createElement("p");
	code.className = "muted";
	code.append("내용 지문 앞 6자 ", textPart("strong", "code"));
	const row = document.createElement("div");
	row.className = "row";
	row.append(verbButton(root, number, "approve", fetcher, resync), verbButton(root, number, "reject", fetcher, resync));
	const result = textPart("p", "result", "muted");
	result.setAttribute("role", "status");
	card.append(title, textPart("p", "summary"), code, row, result);
	root.append(card);
	return card;
}
/**
* 이벤트 한 건을 카드에 반영한다. `pending`이면 그 번호의 카드를 그리거나 고치고(같은 번호는 한 장),
* 아니면 그 카드를 닫는다(지운다). 모양이 맞지 않는 몸통은 무시한다.
*
* (35차 6번) `resync`를 주면 이 카드의 승인 · 거절 단추가 [`decideApproval`]에 그대로 넘긴다(실패 뒤 다시 읽기).
*/
function applyApproval(root, payload, fetcher, resync) {
	const approval = parse(payload);
	if (!approval) return;
	const existing = findCard(root, approval.number);
	if (approval.status !== PENDING$1) {
		existing?.remove();
		return;
	}
	const card = existing ?? newCard(root, approval.number, fetcher, resync);
	const summary = part(card, "summary");
	const code = part(card, "code");
	if (summary) summary.textContent = approval.summary;
	if (code) code.textContent = approval.short_code.slice(0, CODE_LENGTH);
}
function say(card, text) {
	const result = card ? part(card, "result") : null;
	if (result) result.textContent = text;
}
function lockVerbs(card, locked) {
	if (!card) return;
	for (const button of card.querySelectorAll("button[data-verb]")) button.disabled = locked;
}
/**
* 승인 · 거절을 보낸다. 보내는 동안 그 카드의 두 단추를 막고(같은 카드에서 두 번 눌러도 한 번만 나간다),
* 실패하면 다시 켜며, 보냈으면 막힌 채 둔다(이미 결정됨).
*
* 폰과 같이 GET /api/approvals/{id}로 전체 지문을 받아, 그 앞 6자가 카드에 보인
* 지문 앞 6자와 같을 때만 POST /api/approvals/{id}/{verb} `{"fingerprint": 전체 지문}`을 보낸다.
*
* (35차 6번) 결과가 실패(`failed` — 승인 내용 읽기나 답 보내기가 200대가 아님(5xx 포함) · 연결 실패)면 `resync`를
* 한 번 부른다(서버가 이미 닫은 요청의 ✋가 남지 않게 대기 목록을 다시 읽는다). 보냄 · 지문 다름 · 이미 보내는 중이라
* 막힘에는 부르지 않는다. `resync`가 던져도 결과와 단추 상태는 그대로다.
*/
async function decideApproval(root, number, verb, fetcher, resync) {
	const card = findCard(root, number);
	if (card?.dataset.deciding === "1") return "failed";
	if (card) card.dataset.deciding = "1";
	lockVerbs(card, true);
	const outcome = await send$1(card, number, verb, fetcher);
	if (card) delete card.dataset.deciding;
	if (outcome !== "sent") lockVerbs(card, false);
	if (outcome === "failed" && resync) try {
		resync();
	} catch {}
	return outcome;
}
async function send$1(card, number, verb, fetcher) {
	const shown = card ? part(card, "code")?.textContent ?? "" : "";
	try {
		const reply = await fetcher(approvalPath(number), {
			credentials: "same-origin",
			cache: "no-store"
		});
		if (!reply.ok) {
			say(card, "승인 내용을 받지 못했어요: " + reply.status);
			return "failed";
		}
		const body = await reply.json();
		const fingerprint = typeof body.fingerprint === "string" ? body.fingerprint : "";
		if (shown.length !== CODE_LENGTH || fingerprint.slice(0, CODE_LENGTH) !== shown) {
			say(card, "지문이 카드와 달라요. 보내지 않았어요.");
			return "mismatch";
		}
		const sent = await fetcher(decidePath(number, verb), {
			method: "POST",
			credentials: "same-origin",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ fingerprint })
		});
		if (!sent.ok) {
			say(card, "처리하지 못했어요: " + sent.status);
			return "failed";
		}
		say(card, verb === "approve" ? "승인했어요." : "거절했어요.");
		return "sent";
	} catch (error) {
		say(card, "처리하지 못했어요: " + (error instanceof Error ? error.message : String(error)));
		return "failed";
	}
}
// src/office/chat-lines.ts
/** 생각 중 줄의 글. */
var PENDING_TEXT = "생각 중";
/**
* 채팅 줄 하나. 담당 답은 상태가 `complete`이면 그 글, 글이 온 쓰는 중이면 지금까지의 글, 글이 아직 없으면
* (`queued` · `thinking` · 빈 `writing`) [`PENDING_TEXT`] 줄(`pending`)이다. `failed`는 생각 중 줄이 아니다. 이름은 `names`에서 찾고 없으면 id.
*/
function chatLine(entry, names) {
	if (entry.kind === "me") return {
		key: entry.key,
		side: "right",
		name: null,
		text: entry.text,
		tag: null,
		pending: false
	};
	const name = Object.hasOwn(names, entry.agent) ? names[entry.agent] : entry.agent;
	if (entry.kind === "talk") return {
		key: entry.key,
		side: "left",
		name,
		text: entry.text,
		tag: entry.topic,
		pending: false
	};
	const content = entry.content ?? "";
	const waiting = entry.status !== "complete" && entry.status !== "failed" && !content.trim();
	return {
		key: entry.key,
		side: "left",
		name,
		text: waiting ? PENDING_TEXT : content,
		tag: null,
		pending: waiting
	};
}
/** 생각 중 줄의 한 줄 글자: `○○: 생각 중`. */
function pendingLabel(line) {
	return `${line.name ?? ""}: ${PENDING_TEXT}`;
}
// src/office/chat-view.ts
/** 줄 하나. `note`는 이름 옆 작은 덧말(팀 방의 내 글: "→ 받는 담당"), `extra`는 줄에 더 붙일 class. */
function chatLineElement(line, note, extra) {
	const div = document.createElement("div");
	div.className = `msg ${line.side === "right" ? "me" : "agent"}${extra ? ` ${extra}` : ""}`;
	div.dataset.key = line.key;
	if (line.pending) {
		div.classList.add("pending");
		div.textContent = pendingLabel(line);
		return div;
	}
	if (line.name !== null || note) {
		const who = document.createElement("span");
		who.className = "who";
		if (line.name !== null) {
			const name = document.createElement("strong");
			name.textContent = line.name;
			who.append(name);
		}
		if (line.tag) {
			const tag = document.createElement("small");
			tag.className = "topic";
			tag.textContent = line.tag === "day" ? "하루 판단" : line.tag;
			who.append(" ", tag);
		}
		if (note) {
			const small = document.createElement("small");
			small.className = "to";
			small.textContent = note;
			who.append(small);
		}
		div.append(who);
	}
	div.append(document.createTextNode(line.text));
	return div;
}
// src/office/legend.ts
/**
* 지금 쓰는 머리 위 표시 모두(범례 차례): 생각 중 💭 · 쓰는 중 💬 · 말하는 중 ··· · 기다림 ⏳ · 실패 ❗ ·
* 승인 기다림 ✋ · 끝남 ✓. 글자는 지금 화면이 쓰는 것과 같다(speech.ts `STATE_ICONS` · `TYPING_TEXT`는 이것을 쓴다).
*/
var OVERHEAD_MARKS = [
	{
		id: "thinking",
		glyph: "💭",
		color: "#5a6b5e",
		label: "생각 중"
	},
	{
		id: "writing",
		glyph: "💬",
		color: "#4e745b",
		label: "답을 쓰는 중"
	},
	{
		id: "typing",
		glyph: "···",
		color: "#3e513f",
		label: "말하는 중"
	},
	{
		id: "waiting",
		glyph: "⏳",
		color: "#8a5a00",
		label: "차례를 기다리는 중"
	},
	{
		id: "failed",
		glyph: "❗",
		color: "#b3412f",
		label: "일이 막힘(실패)"
	},
	{
		id: "approval",
		glyph: "✋",
		color: "#a4561d",
		label: "내 승인을 기다림 — 누르면 승인 카드가 열려요"
	},
	{
		id: "done",
		glyph: "✓",
		color: "#2f6b47",
		label: "일을 마침(잠깐 보였다 사라져요)"
	}
];
var legendCount = 0;
var BY_ID = new Map(OVERHEAD_MARKS.map((mark) => [mark.id, mark]));
/** 그 종류의 표시. */
function markOf(id) {
	const mark = BY_ID.get(id);
	if (!mark) throw new Error(`모르는 표시: ${id}`);
	return mark;
}
/** 범례를 `root` 아래에 그린다(위 모양, 처음에는 닫힘). 범례 요소(`[data-legend]`)를 돌려준다. */
function renderLegend(root) {
	const toggle = document.createElement("button");
	toggle.type = "button";
	toggle.className = "legend-toggle";
	toggle.dataset.legendToggle = "";
	toggle.textContent = "표시 뜻";
	toggle.title = "머리 위 표시의 뜻";
	toggle.setAttribute("aria-expanded", "false");
	const legend = document.createElement("div");
	legend.className = "legend";
	legend.dataset.legend = "";
	legendCount += 1;
	legend.id = `overhead-legend-${legendCount}`;
	legend.setAttribute("role", "region");
	legend.setAttribute("aria-label", "머리 위 표시의 뜻");
	legend.hidden = true;
	toggle.setAttribute("aria-controls", legend.id);
	const list = document.createElement("ul");
	for (const mark of OVERHEAD_MARKS) {
		const row = document.createElement("li");
		row.dataset.mark = mark.id;
		const glyph = document.createElement("span");
		glyph.dataset.part = "glyph";
		glyph.className = "legend-glyph";
		glyph.textContent = mark.glyph;
		glyph.style.color = mark.color;
		glyph.setAttribute("aria-hidden", "true");
		const label = document.createElement("span");
		label.dataset.part = "label";
		label.textContent = mark.label;
		row.append(glyph, label);
		list.append(row);
	}
	legend.append(list);
	toggle.addEventListener("click", () => {
		legend.hidden = !legend.hidden;
		toggle.setAttribute("aria-expanded", String(!legend.hidden));
	});
	root.append(toggle, legend);
	return legend;
}
/** 글 말풍선이 마지막으로 바뀐 뒤 보이는 시간(밀리초). */
var SPEECH_LIFE_MS = 12e3;
/** 글 없이 "말하는 중"만 알 때의 표시 글. */
var TYPING_TEXT = markOf("typing").glyph;
/** [`TYPING_TEXT`] 말풍선이 보이는 시간(밀리초). */
var TYPING_LIFE_MS = 4e3;
/** 좌우 후보가 비켜나는 거리(말풍선 폭의 배수). */
var BUBBLE_SIDE_SHIFT = .55;
/** 말풍선 넓이의 합이 화면 넓이의 이 비율을 넘으면 모두 말풍선 줄로 보낸다. */
var LANE_AREA_RATIO = .24;
/** 상태 표시: 생각 중 💭 · 쓰는 중 💬 · 기다림 ⏳ · 실패 ❗. 글 앞에 붙인다. */
var STATE_ICONS = {
	thinking: markOf("thinking").glyph,
	writing: markOf("writing").glyph,
	waiting: markOf("waiting").glyph,
	failed: markOf("failed").glyph
};
/** 빈칸(줄바꿈 포함)을 하나로 접고 앞뒤를 깎은 뒤, [`SPEECH_MAX`]자를 넘으면 [`SPEECH_CUT`]자 + "..."로 자른다. */
function trimSpeech(text) {
	const folded = text.replace(/\s+/g, " ").trim();
	const chars = Array.from(folded);
	if (chars.length <= 240) return folded;
	return `${chars.slice(0, 237).join("")}...`;
}
/** 말풍선 폭(px): `max(80, min(190, 화면폭 - 16))`. */
function bubbleWidth(screenWidth) {
	return Math.max(80, Math.min(190, screenWidth - 16));
}
/**
* 담당마다의 말풍선 판. 한 담당에 종류마다 하나씩 두고, 보일 때는 우선순위가 가장 높은 하나만 보인다.
*/
var SpeechBoard = class {
	/** 담당 id → 종류 → 말풍선. */
	bubbles = /* @__PURE__ */ new Map();
	put(bubble) {
		let mine = this.bubbles.get(bubble.agent);
		if (!mine) {
			mine = /* @__PURE__ */ new Map();
			this.bubbles.set(bubble.agent, mine);
		}
		mine.set(bubble.kind, bubble);
	}
	live(agent, kind, now) {
		const bubble = this.bubbles.get(agent)?.get(kind);
		return bubble && now < bubble.until ? bubble : void 0;
	}
	/**
	* 대화 말풍선(논의 글 · 담당 답)을 둔다. 같은 담당의 대화 말풍선은 통째로 바꾼다(스트리밍 중에는 지금까지의 글
	* 전체를 넘긴다 — 덧붙이지 않는다). 수명은 `now`부터 [`SPEECH_LIFE_MS`]. `state`가 있으면 그 표시를 글 앞에 붙인다.
	*/
	say(agent, text, now, state) {
		const shown = state ? trimSpeech(`${STATE_ICONS[state]} ${text}`) : trimSpeech(text);
		this.put({
			agent,
			kind: "talk",
			text: shown,
			until: now + SPEECH_LIFE_MS
		});
	}
	/** 글 없이 "말하는 중"만 알 때: 대화 말풍선을 [`TYPING_TEXT`]로 [`TYPING_LIFE_MS`] 동안 둔다. */
	typing(agent, now) {
		this.put({
			agent,
			kind: "talk",
			text: TYPING_TEXT,
			until: now + TYPING_LIFE_MS
		});
	}
	/** 상태 말풍선(표시만)을 둔다. 지울 때까지 보인다. */
	status(agent, state, now) {
		this.put({
			agent,
			kind: "status",
			text: STATE_ICONS[state],
			until: Number.POSITIVE_INFINITY
		});
	}
	/** 잡담 말풍선을 `until`까지 둔다. */
	smalltalk(agent, text, now, until) {
		this.put({
			agent,
			kind: "smalltalk",
			text: trimSpeech(text),
			until
		});
	}
	/** 그 담당의 말풍선을 지운다(`kind`가 없으면 모두). */
	clear(agent, kind) {
		if (kind === void 0) this.bubbles.delete(agent);
		else this.bubbles.get(agent)?.delete(kind);
	}
	/** `now`에 그 담당에게 보이는 말풍선(우선순위가 가장 높은 하나). 없으면 `undefined`. */
	shown(agent, now) {
		return this.live(agent, "talk", now) ?? this.live(agent, "status", now) ?? this.live(agent, "smalltalk", now);
	}
	/** `now`에 대화나 상태 말풍선이 있는가(있으면 잡담하지 않는다). */
	busy(agent, now) {
		return this.live(agent, "talk", now) !== void 0 || this.live(agent, "status", now) !== void 0;
	}
};
/**
* (23차 2번) 머리 위 말풍선의 3×3 후보(원본 `label-layout` 순서): 위아래 0 · 위로 `높이 + 6` · 아래로 `높이 + 6`,
* 각 줄 안에서 좌우 0 · -0.55폭 · +0.55폭. 첫 자리는 가운데 맞춤, 위쪽 끝 `max(8, 머리 y - 높이 - 8)`.
*/
function bubbleCandidates(headX, headY, size) {
	const { width, height } = size;
	const x0 = headX - width / 2;
	const y0 = Math.max(8, headY - height - 8);
	const shift = width * BUBBLE_SIDE_SHIFT;
	const step = height + 6;
	const out = [];
	for (const dy of [
		0,
		-step,
		step
	]) for (const dx of [
		0,
		-shift,
		shift
	]) out.push({
		x: x0 + dx,
		y: y0 + dy,
		width,
		height
	});
	return out;
}
/** 사각형이 화면 안에 다 들어오는가. */
function insideScreen(r, screen) {
	return r.x >= 0 && r.y >= 0 && r.x + r.width <= screen.width && r.y + r.height <= screen.height;
}
/** 두 사각형이 겹치는가(맞닿음은 겹침이 아니다). */
function overlap(a, b) {
	return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}
/**
* 가벼운 화면(WebGL 없음)에서 담당 줄 아래 한 줄: 그 담당의 가장 최근 말이 [`LATEST_LIFE_MS`] 안이면
* [`trimSpeech`]를 거친 글, 아니면 `null`.
*/
function latestLine(said, agent, now) {
	let latest;
	for (const one of said) if (one.agent === agent && (!latest || one.at >= latest.at)) latest = one;
	if (!latest || now - latest.at >= 12e3) return null;
	return trimSpeech(latest.text);
}
/** (23차 2번) 말풍선 글의 이름 구분자(원본: 이름을 따로 굵게 하지 않는 한 줄 글자). */
var NAME_SEPARATOR = " · ";
/**
* (23차 2번 · 3번) 말풍선에 보일 한 줄 글자: `이름 · 글`. 상태가 붙은 글이면 `이름 · 💭 글`, 글 없이 말하는 중이면
* `이름 · ···`, 글 없는 상태 말풍선이면 `이름 · 💭`. `shown`은 [`SpeechBoard.shown`]이 낸 것이다.
*/
function bubbleText(name, shown) {
	return `${name}${NAME_SEPARATOR}${shown.text}`;
}
/**
* (23차 3번) 보이는 말풍선이 어디에 그려지나: 글 말풍선(대화 · "···" · 잡담)은 머리 위, 글 없는 상태 말풍선(생각 중 ·
* 기다림 · 실패 표시만)은 말풍선 줄, 말풍선이 없으면(쉬는 중 포함) 아무 데도.
*/
function bubbleWhere(shown) {
	if (!shown) return "none";
	return shown.kind === "status" ? "rail" : "head";
}
/**
* 장면 아래 가운데 한 줄 자막: 모든 담당의 말 가운데 가장 최근 것(같은 시각이면 뒤에 넘긴 것)이 [`CAPTION_LIFE_MS`]
* 안이면 `이름: 글`(글은 `trimSpeech`를 거친 것), 아니면 `null`(숨긴다). 이름은 `names`에서 찾고 없으면 id.
*/
function caption(said, names, now) {
	const last = latest(said);
	if (!last || now - last.at >= 12e3) return null;
	return `${nameOf$1(names, last.agent)}: ${trimSpeech(last.text)}`;
}
/** 가장 최근 것(같은 시각이면 뒤에 넘긴 것). */
function latest(list) {
	let found;
	for (const one of list) if (!found || one.at >= found.at) found = one;
	return found;
}
function nameOf$1(names, agent) {
	return Object.hasOwn(names, agent) ? names[agent] : agent;
}
/**
* 팀 탭 위 한 줄: 마지막 논의 글이 [`SPEAKING_LIFE_MS`] 안이면 `○○ 발언 중…`(그 글의 담당 이름), 아니면 `null`.
*/
function speakingLine(talks, names, now) {
	const last = latest(talks);
	if (!last || now - last.at >= 4e3) return null;
	return `${nameOf$1(names, last.agent)} 발언 중…`;
}
// src/office/team-feed.ts
/** `office.talk` 하나를 팀 탭 것으로. `key`는 이벤트 번호로 만든다(같은 번호는 같은 줄). */
function talkEntry(talk, seq) {
	return {
		kind: "talk",
		key: `talk-${seq}`,
		at: talk.at,
		agent: talk.agent,
		text: talk.text,
		topic: talk.topic
	};
}
/**
* 팀 탭 줄들: `key`가 겹치면 처음 것만 남기고, 시각(`at`) 순서로(같은 시각이면 넘긴 순서) 늘어놓는다.
* 담당 이름은 `names`(담당 id → 이름)에서 찾고, 없으면 id를 그대로 쓴다.
*/
function teamFeed(entries, names) {
	const seen = /* @__PURE__ */ new Set();
	const kept = [];
	entries.forEach((entry, order) => {
		if (seen.has(entry.key)) return;
		seen.add(entry.key);
		kept.push({
			entry,
			order
		});
	});
	kept.sort((a, b) => a.entry.at - b.entry.at || a.order - b.order);
	return kept.map(({ entry }) => {
		const name = Object.hasOwn(names, entry.agent) ? names[entry.agent] : entry.agent;
		return {
			kind: entry.kind,
			key: entry.key,
			at: entry.at,
			who: entry.kind === "me" ? `나 → ${name}` : name,
			text: entry.text,
			tag: entry.kind === "talk" ? entry.topic : null
		};
	});
}
/** 팀 탭에 쥐는 논의 줄 수(채팅 줄 정리와 같은 200줄). */
var KEEP_TALKS = 200;
/** 가벼운 화면을 위해 쥐는 최근 말 수. */
var KEEP_SAID = 60;
/** 방 로그의 쓴 쪽 id → 사무실 담당 id(뉴스 공시 시세 담당은 방 로그에서 `scout`, 사무실에서 `news`). */
function officeAgent(id) {
	return id === "scout" ? "news" : id;
}
/** `office.talk` 내용을 읽는다(모양이 틀리면 `null`). 담당 id는 사무실 id로 바꾼다. */
function readTalk(payload) {
	if (typeof payload !== "object" || payload === null) return null;
	const p = payload;
	if (typeof p.agent !== "string" || typeof p.text !== "string" || typeof p.at !== "number") return null;
	return {
		room: typeof p.room === "string" ? p.room : "",
		agent: officeAgent(p.agent),
		text: p.text,
		at: p.at,
		topic: typeof p.topic === "string" ? p.topic : ""
	};
}
/** 팀의 대화 상태. */
var Chatter = class {
	/** 머리 위 말풍선 판(3D 장면이 그린다). */
	board = new SpeechBoard();
	said = [];
	talks = [];
	/** (23차 7번 (가)) 실시간으로 받은 논의 글의 담당 · 받은 시각(발언 중 표시). */
	marks = [];
	/** 팀 탭 줄의 처음 본 시각(내 글 · 담당 답). */
	stamps = /* @__PURE__ */ new Map();
	/** `office.talk` 하나. `live`면(실시간으로 온 것) 말풍선 · 최근 말에도 올린다. */
	talk(payload, seq, live, now = Date.now()) {
		const talk = readTalk(payload);
		if (!talk) return false;
		const entry = talkEntry(talk, seq);
		if (this.talks.some((t) => t.key === entry.key)) return false;
		this.talks.push(entry);
		if (this.talks.length > KEEP_TALKS) this.talks.splice(0, this.talks.length - KEEP_TALKS);
		if (live) {
			this.board.say(talk.agent, talk.text, now);
			this.remember(talk.agent, talk.text, now);
			this.marks.push({
				agent: talk.agent,
				at: now
			});
			if (this.marks.length > KEEP_SAID) this.marks.splice(0, this.marks.length - KEEP_SAID);
		}
		return true;
	}
	/** 담당 답의 상태: 쓰는 중이면 지금까지의 글 전체로 바꾸고, 끝나면 글로, 실패면 ❗. */
	chat(resp, now = Date.now()) {
		const content = resp.content ?? "";
		if (resp.status === "complete") {
			this.board.clear(resp.agent, "status");
			if (content.trim()) {
				this.board.say(resp.agent, content, now);
				this.remember(resp.agent, content, now);
			}
		} else if (resp.status === "failed") this.board.say(resp.agent, "답하지 못했습니다", now, "failed");
		else if (content.trim()) this.board.say(resp.agent, content, now, "writing");
		else if (resp.status === "thinking") this.board.typing(resp.agent, now);
	}
	/** 담당의 일(`office.agent`의 `work`)을 상태 말풍선으로. */
	work(agent, work, now = Date.now()) {
		const state = {
			queued: "waiting",
			thinking: "thinking",
			failed: "failed"
		}[work];
		if (state) this.board.status(agent, state, now);
		else this.board.clear(agent, "status");
	}
	remember(agent, text, at) {
		this.said.push({
			agent,
			text,
			at
		});
		if (this.said.length > KEEP_SAID) this.said.splice(0, this.said.length - KEEP_SAID);
	}
	/** 가벼운 화면에서 그 담당 줄 아래 한 줄(없으면 `null`). */
	latest(agent, now = Date.now()) {
		return latestLine(this.said, agent, now);
	}
	/** 팀 탭 줄(내 글 · 담당 답)의 시각: 처음 본 때. */
	stamp(key, now = Date.now()) {
		const at = this.stamps.get(key);
		if (at !== void 0) return at;
		this.stamps.set(key, now);
		return now;
	}
	/** (23차 6번) 화면 아래 자막: 가장 최근 말 `이름: 글`(12초), 없으면 `null`. */
	caption(names, now = Date.now()) {
		return caption(this.said, names, now);
	}
	/** (23차 7번 (가)) 팀 탭 위 `○○ 발언 중…`(마지막 논의 글 뒤 4초), 없으면 `null`. */
	speaking(names, now = Date.now()) {
		return speakingLine(this.marks, names, now);
	}
	/** 팀 탭에 섞을 논의 줄들. */
	talkEntries() {
		return this.talks;
	}
};
// src/office/looks.ts
/** 원본 분류(`LOOK_CATEGORIES`, "전체" 제외). */
var LOOK_CATEGORIES = [
	{
		id: "classic",
		label: "클래식 수트"
	},
	{
		id: "casual",
		label: "비즈니스 캐주얼"
	},
	{
		id: "creative",
		label: "크리에이티브"
	},
	{
		id: "leadership",
		label: "리더십"
	}
];
var LOOKS = [
	{
		id: "office-jun",
		name: "서준",
		category: "classic",
		subtitle: "첫 출근의 설렘 · 네이비 수트와 사원증"
	},
	{
		id: "office-tae",
		name: "태오",
		category: "casual",
		subtitle: "현장을 아는 베테랑 · 셔츠와 느슨한 타이"
	},
	{
		id: "office-seo",
		name: "서린",
		category: "classic",
		subtitle: "차분한 추진력 · 차콜 수트와 포니테일"
	},
	{
		id: "office-min",
		name: "민재",
		category: "classic",
		subtitle: "정돈된 취향 · 체크 수트와 둥근 안경"
	},
	{
		id: "office-do",
		name: "도윤",
		category: "casual",
		subtitle: "든든한 동료 · 니트 베스트와 셔츠"
	},
	{
		id: "office-yun",
		name: "윤서",
		category: "leadership",
		subtitle: "단단한 리더 · 네이비 재킷과 와이드 팬츠"
	},
	{
		id: "office-ha",
		name: "하경",
		category: "creative",
		subtitle: "선명한 아이디어 · 머스터드 니트와 와이드 팬츠"
	},
	{
		id: "office-jin",
		name: "진우",
		category: "creative",
		subtitle: "책을 만드는 사람 · 카멜 코트와 터틀넥"
	},
	{
		id: "office-eun",
		name: "은채",
		category: "casual",
		subtitle: "따뜻한 집중력 · 카디건과 플리츠 롱스커트"
	},
	{
		id: "office-hyeon",
		name: "현진",
		category: "leadership",
		subtitle: "경험이 만든 여유 · 실버 헤어와 다크 쓰리피스"
	},
	{
		id: "office-nari",
		name: "나리",
		category: "creative",
		subtitle: "광고 기획자 · 리본 블라우스와 무릎 스커트"
	},
	{
		id: "office-roan",
		name: "로안",
		category: "creative",
		subtitle: "아트디렉터 · 검은 터틀넥과 체크 코트"
	},
	{
		id: "office-soi",
		name: "소이",
		category: "creative",
		subtitle: "브랜드 전략가 · 올림머리와 더블 재킷"
	},
	{
		id: "office-yul",
		name: "율",
		category: "creative",
		subtitle: "카피라이터 · 곱슬머리와 니트 베스트"
	},
	{
		id: "office-bomi",
		name: "보미",
		category: "creative",
		subtitle: "콘텐츠 프로듀서 · 땋은 머리와 긴 치마"
	},
	{
		id: "office-jiho",
		name: "지호",
		category: "casual",
		subtitle: "촬영 감독 · 셔츠와 현장 헤드셋"
	},
	{
		id: "office-dami",
		name: "다미",
		category: "creative",
		subtitle: "캠페인 매니저 · 스카프와 짧은 단발"
	},
	{
		id: "office-seul",
		name: "슬아",
		category: "creative",
		subtitle: "디자인 리서처 · 긴 머리와 넉넉한 니트"
	},
	{
		id: "office-kyu",
		name: "규민",
		category: "classic",
		subtitle: "클라이언트 파트너 · 줄무늬 수트와 타이"
	},
	{
		id: "office-ara",
		name: "아라",
		category: "creative",
		subtitle: "행사 디렉터 · 포니테일과 활동적인 베스트"
	},
	{
		id: "office-ian",
		name: "이안",
		category: "casual",
		subtitle: "백엔드 개발자 · 안경과 후드 집업"
	},
	{
		id: "office-rumi",
		name: "루미",
		category: "casual",
		subtitle: "프런트엔드 개발자 · 땋은 머리와 후드"
	},
	{
		id: "office-gonu",
		name: "건우",
		category: "casual",
		subtitle: "QA 엔지니어 · 체크 셔츠와 기록 노트"
	},
	{
		id: "office-haena",
		name: "해나",
		category: "casual",
		subtitle: "데이터 과학자 · 연구 가운과 올림머리"
	},
	{
		id: "office-woojin",
		name: "우진",
		category: "casual",
		subtitle: "사내 IT 담당 · 곱슬머리와 넓은 바지"
	},
	{
		id: "office-jua",
		name: "주아",
		category: "creative",
		subtitle: "UX 디자이너 · 물결 머리와 리본 니트"
	},
	{
		id: "office-taemin",
		name: "태민",
		category: "casual",
		subtitle: "스크럼 마스터 · 베스트와 사원증"
	},
	{
		id: "office-sera",
		name: "세라",
		category: "casual",
		subtitle: "보안 엔지니어 · 긴 머리와 터틀넥 코트"
	},
	{
		id: "office-hosu",
		name: "호수",
		category: "casual",
		subtitle: "로봇 연구원 · 짧은 머리와 실험 가운"
	},
	{
		id: "office-yena",
		name: "예나",
		category: "creative",
		subtitle: "커뮤니티 매니저 · 단발과 넉넉한 후드"
	},
	{
		id: "office-sungho",
		name: "성호",
		category: "leadership",
		subtitle: "운영 총괄 · 실버 가르마와 더블 수트"
	},
	{
		id: "office-hyejin",
		name: "혜진",
		category: "leadership",
		subtitle: "재무 책임자 · 올림머리와 차분한 스카프"
	},
	{
		id: "office-jungwon",
		name: "정원",
		category: "leadership",
		subtitle: "인사 책임자 · 은빛 단발과 긴 니트"
	},
	{
		id: "office-seok",
		name: "석현",
		category: "leadership",
		subtitle: "사업 대표 · 웨이브 헤어와 체크 수트"
	},
	{
		id: "office-mira",
		name: "미라",
		category: "leadership",
		subtitle: "크리에이티브 총괄 · 긴 머리와 더블 재킷"
	},
	{
		id: "office-kyung",
		name: "경수",
		category: "leadership",
		subtitle: "법무 책임자 · 정돈된 머리와 쓰리피스"
	},
	{
		id: "office-yeon",
		name: "연화",
		category: "leadership",
		subtitle: "이사회 의장 · 실버 올림머리와 코트"
	},
	{
		id: "office-dohun",
		name: "도훈",
		category: "leadership",
		subtitle: "창업자 · 곱슬머리와 실용적인 후드"
	},
	{
		id: "office-suhye",
		name: "수혜",
		category: "leadership",
		subtitle: "영업 총괄 · 포니테일과 선명한 수트"
	},
	{
		id: "office-jaewon",
		name: "재원",
		category: "leadership",
		subtitle: "전략 고문 · 은빛 웨이브와 터틀넥"
	},
	{
		id: "office-daeun",
		name: "다은",
		category: "creative",
		subtitle: "문학 편집자 · 긴 머리와 긴 스커트"
	},
	{
		id: "office-jiseok",
		name: "지석",
		category: "creative",
		subtitle: "번역 편집자 · 곱슬머리와 체크 베스트"
	},
	{
		id: "office-seona",
		name: "선아",
		category: "creative",
		subtitle: "북디자이너 · 땋은 머리와 아틀리에 코트"
	},
	{
		id: "office-haram",
		name: "하람",
		category: "classic",
		subtitle: "해외 판권 담당 · 스카프와 더블 수트"
	},
	{
		id: "office-chan",
		name: "찬영",
		category: "casual",
		subtitle: "제작 관리 · 셔츠와 공방 사원증"
	},
	{
		id: "office-eunsol",
		name: "은솔",
		category: "creative",
		subtitle: "교정 교열자 · 올림머리와 니트 카디건"
	},
	{
		id: "office-sejin",
		name: "세진",
		category: "classic",
		subtitle: "협상 전문가 · 가르마와 핀스트라이프"
	},
	{
		id: "office-hyo",
		name: "효주",
		category: "casual",
		subtitle: "서점 영업 · 짧은 단발과 편안한 베스트"
	},
	{
		id: "office-yumin",
		name: "유민",
		category: "classic",
		subtitle: "파트너십 매니저 · 웨이브와 노타이 수트"
	},
	{
		id: "office-garam",
		name: "가람",
		category: "creative",
		subtitle: "작가 매니저 · 포니테일과 넉넉한 코트"
	}
];
/** 담당별 기본 외형(여섯 모두 다르다). 서버 `reach/looks_http.rs`의 기본값과 같다. */
var DEFAULT_LOOKS = {
	news: "office-seo",
	strategy: "office-jaewon",
	risk: "office-sera",
	review: "office-min",
	trader: "office-sejin",
	leader: "office-yun"
};
/** id로 외형 정보를 찾는다(없으면 undefined). */
function lookInfo(id) {
	return LOOKS.find((look) => look.id === id);
}
// src/office/look-picker.ts
/** 외형 이름(모르면 id). */
function lookName(id) {
	const info = lookInfo(id);
	return info ? info.name : id;
}
/** 외형 고르기 칸을 만든다. 돌려준 함수로 연다(담당을 정해 열 수 있다). */
function createLookPicker(o) {
	let agent = o.agents[o.agents.length - 1];
	let busy = false;
	let message = "";
	function render() {
		const looks = o.current();
		o.panel.replaceChildren();
		const head = document.createElement("div");
		head.className = "looks-head";
		const title = document.createElement("strong");
		title.textContent = "담당 외형";
		const close = document.createElement("button");
		close.type = "button";
		close.textContent = "닫기";
		close.addEventListener("click", () => {
			o.panel.hidden = true;
		});
		head.append(title, close);
		const tabs = document.createElement("div");
		tabs.className = "looks-agents";
		tabs.setAttribute("role", "tablist");
		for (const id of o.agents) {
			const b = document.createElement("button");
			b.type = "button";
			b.setAttribute("role", "tab");
			b.setAttribute("aria-selected", String(id === agent));
			b.textContent = `${o.nameOf(id)} · ${lookName(looks[id] ?? DEFAULT_LOOKS[id] ?? "")}`;
			b.addEventListener("click", () => {
				agent = id;
				message = "";
				render();
			});
			tabs.append(b);
		}
		const note = document.createElement("p");
		note.className = "looks-note";
		note.setAttribute("role", "status");
		note.textContent = message;
		o.panel.append(head, tabs, note);
		for (const category of LOOK_CATEGORIES) {
			const group = document.createElement("section");
			group.className = "looks-group";
			const h = document.createElement("h3");
			h.textContent = category.label;
			const list = document.createElement("div");
			list.className = "looks-list";
			for (const look of LOOKS.filter((l) => l.category === category.id)) {
				const b = document.createElement("button");
				b.type = "button";
				b.className = "look";
				b.disabled = busy;
				b.setAttribute("aria-pressed", String(looks[agent] === look.id));
				const name = document.createElement("strong");
				name.textContent = look.name;
				const sub = document.createElement("span");
				sub.textContent = look.subtitle;
				b.append(name, sub);
				b.addEventListener("click", () => {
					pick(look.id);
				});
				list.append(b);
			}
			group.append(h, list);
			o.panel.append(group);
		}
	}
	async function pick(look) {
		if (busy) return;
		busy = true;
		message = "바꾸는 중…";
		render();
		try {
			const looks = await o.choose(agent, look);
			o.applied(looks);
			message = `${o.nameOf(agent)}: ${lookName(look)}(으)로 바꿨습니다.`;
		} catch {
			message = "바꾸지 못했습니다(연결을 확인하세요).";
		}
		busy = false;
		render();
	}
	return {
		open(which) {
			if (which && o.agents.includes(which)) agent = which;
			message = "";
			o.panel.hidden = false;
			render();
		},
		close() {
			o.panel.hidden = true;
		}
	};
}
// src/office/follow.ts
/** 한 프레임에 보는 점이 캐릭터 쪽으로 다가가는 몫(원본 0.08). */
var FOLLOW_LERP = .08;
/** 따라가기 켜짐 · 꺼짐 상태. 기본 켜짐. */
var FollowState = class {
	following = true;
	listeners = /* @__PURE__ */ new Set();
	/** 따라가는 중인가. */
	on() {
		return this.following;
	}
	/** 사용자가 카메라를 손으로 움직였다: 따라가기를 푼다. */
	touch(kind) {
		this.set(false);
	}
	/** "따라가기" 단추: 다시 켠다. */
	resume() {
		this.set(true);
	}
	/** 켜짐 · 꺼짐이 바뀔 때 부른다(도구 줄 단추 · 발언자 카메라에 알린다). 끊는 함수를 돌려준다. */
	onChange(listener) {
		this.listeners.add(listener);
		return () => {
			this.listeners.delete(listener);
		};
	}
	set(on) {
		if (on === this.following) return;
		this.following = on;
		for (const listener of [...this.listeners]) listener(on);
	}
};
/** 한 프레임의 따라가기: 보는 점을 캐릭터 쪽으로 `factor`만큼 옮긴 새 점. */
function followStep(target, player, factor = FOLLOW_LERP) {
	return {
		x: target.x + (player.x - target.x) * factor,
		z: target.z + (player.z - target.z) * factor
	};
}
// src/office/approval-bubbles.ts
var AGENTS = [
	"news",
	"strategy",
	"risk",
	"review",
	"trader",
	"leader"
];
var LEADER = "leader";
var PENDING = "pending";
var WAITING = "waiting";
var CLOSED_STATES = /* @__PURE__ */ new Set(["answered", "expired"]);
function agentOf(value) {
	return typeof value === "string" && AGENTS.includes(value) ? value : LEADER;
}
function record$1(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}
function text(value) {
	return typeof value === "string" ? value : "";
}
/** 이벤트가 기다림인가(`state`가 있으면 그것, 없으면 `status`). 모양이 맞지 않으면 `null`. */
function waitingOf(p) {
	if (typeof p.state === "string") {
		if (p.state === WAITING) return true;
		if (CLOSED_STATES.has(p.state)) return false;
	}
	if (typeof p.status === "string") return p.status === PENDING;
	return null;
}
/** 승인 기다림 상태. */
var ApprovalBubbles = class {
	items = /* @__PURE__ */ new Map();
	/** `office.approval` 이벤트 하나를 반영한다(위 규칙). */
	apply(payload, now) {
		const p = record$1(payload);
		if (!p || typeof p.number !== "string" || p.number === "") return "ignored";
		const waiting = waitingOf(p);
		if (waiting === null) return "ignored";
		const number = p.number;
		if (!waiting) {
			this.items.delete(number);
			return "closed";
		}
		const known = this.items.get(number);
		if (known) {
			if (typeof p.summary === "string") known.summary = p.summary;
			if (typeof p.short_code === "string") known.short_code = p.short_code;
			return "known";
		}
		this.items.set(number, {
			number,
			agent: agentOf(p.agent),
			at: now,
			summary: text(p.summary),
			short_code: text(p.short_code)
		});
		return "new";
	}
	/**
	* 다시 읽은 `pending_approvals`로 통째로 맞춘다: 목록에 없는 것은 지우고 있는 것은 넣거나 고친다. 모양이 맞지 않는
	* 줄은 건너뛴다. 배열이 아니면(그 칸이 없으면) 아무것도 바꾸지 않는다. 새로 넣은 것도 "새 요청"으로 치지 않는다.
	*/
	sync(list) {
		if (!Array.isArray(list)) return;
		const next = /* @__PURE__ */ new Map();
		for (const line of list) {
			const p = record$1(line);
			if (!p || typeof p.number !== "string" || p.number === "") continue;
			const at = typeof p.created_at === "number" && Number.isFinite(p.created_at) ? p.created_at : null;
			const known = this.items.get(p.number);
			if (at === null && !known) continue;
			next.set(p.number, {
				number: p.number,
				agent: agentOf(p.agent),
				at: at ?? known.at,
				summary: typeof p.summary === "string" ? p.summary : known?.summary ?? "",
				short_code: typeof p.short_code === "string" ? p.short_code : known?.short_code ?? ""
			});
		}
		this.items.clear();
		for (const [number, item] of next) this.items.set(number, item);
	}
	/** 지금 기다리는 승인 모두(오래된 것부터). */
	waiting() {
		return [...this.items.values()].sort((a, b) => a.at - b.at || (a.number < b.number ? -1 : a.number > b.number ? 1 : 0)).map((item) => ({ ...item }));
	}
	/** 그 담당의 ✋ 말풍선. 기다리는 것이 없으면 `undefined`. 시간으로 사라지지 않는다(`now`와 무관). */
	bubble(agent, now) {
		const mine = this.waiting().filter((item) => item.agent === agent);
		if (mine.length === 0) return void 0;
		const mark = markOf("approval");
		return {
			agent,
			oldest: mine[0],
			count: mine.length,
			text: mine.length > 1 ? `${mark.glyph} ${mine.length}` : mark.glyph,
			color: mark.color
		};
	}
	/** ✋가 떠 있는 담당 모두. */
	bubbles(now) {
		const out = [];
		for (const agent of AGENTS) {
			const hand = this.bubble(agent, now);
			if (hand) out.push(hand);
		}
		return out;
	}
	/**
	* 그 담당의 ✋를 눌렀다: 가장 오래된 요청으로 `open`(승인 카드 열기)을 한 번 부르고 `true`. ✋가 없으면 부르지 않고
	* `false`. 누른 것만으로는 ✋가 사라지지 않는다(답하거나 만료될 때까지 남는다).
	*/
	press(agent, open) {
		const hand = this.bubble(agent, 0);
		if (!hand) return false;
		open(hand.oldest);
		return true;
	}
};
// src/office/chime.ts
/** 알림음의 음들(원본 권한 알림음: A5 → E5). */
var CHIME_NOTES = [{
	hz: 880,
	at: 0
}, {
	hz: 659.25,
	at: .12
}];
/** 음마다 길이(초). */
var CHIME_NOTE_SECONDS = .15;
/** 음량(0~1). */
var CHIME_VOLUME = .12;
/** 음이 사라질 때까지 줄이는 음량(지수 램프는 0으로 갈 수 없다). */
var CHIME_FLOOR = 1e-4;
/** 음 시작의 짧은 올림(초) — 딸깍 소리를 막는다. */
var CHIME_ATTACK = .01;
/** 승인 알림음. 오디오는 처음 필요할 때 한 번 만든다. */
var Chime = class {
	makeAudio;
	enabled;
	audio = null;
	/** 낸 번호 · 다시 읽어 채운 번호(다시 내지 않는다). */
	heard = /* @__PURE__ */ new Set();
	/** `makeAudio`: 오디오 만들기. `enabled`: 알림음 설정을 읽는다(화면은 `soundEnabled`). */
	constructor(makeAudio, enabled) {
		this.makeAudio = makeAudio;
		this.enabled = enabled;
	}
	/** 오디오(처음이면 만든다). 만들 수 없으면 `null`. */
	context() {
		if (this.audio) return this.audio;
		try {
			this.audio = this.makeAudio();
		} catch {
			this.audio = null;
		}
		return this.audio;
	}
	/** 설정을 읽는다(읽지 못하면 끈 것으로 본다). */
	async allowed() {
		try {
			return await this.enabled() === true;
		} catch {
			return false;
		}
	}
	/** 오디오가 돌고 있게 한다(막혀 있으면 `resume`). 돌면 `true`. */
	async running(audio) {
		if (audio.state === "running") return true;
		try {
			await audio.resume();
		} catch {
			return false;
		}
		return audio.state === "running";
	}
	/** 화면을 눌렀다(사용자 손짓): 막힌 오디오를 푼다(`resume`). 실패해도 던지지 않는다. */
	async unlock() {
		if (!await this.allowed()) return;
		const audio = this.context();
		if (!audio) return;
		await this.running(audio);
	}
	/** 다시 읽어 채운 번호들: 소리 없이 "이미 알린 것"으로 적는다. */
	quiet(numbers) {
		for (const number of numbers) this.heard.add(number);
	}
	/**
	* 새 승인 요청 `number`에 알림음을 낸다. 낸 번호 · 채운 번호면 내지 않는다. 설정이 꺼져 있거나 소리가 막히면 내지
	* 않는다. 소리를 냈으면 `true`, 아니면 `false`. 던지지 않는다.
	*/
	async ring(number) {
		if (this.heard.has(number)) return false;
		this.heard.add(number);
		if (!await this.allowed()) return false;
		const audio = this.context();
		if (!audio || !await this.running(audio)) return false;
		try {
			this.play(audio);
			return true;
		} catch {
			return false;
		}
	}
	/** 사인파 두 음을 낸다(A5 → E5, 음마다 [`CHIME_NOTE_SECONDS`], 음량 [`CHIME_VOLUME`]). */
	play(audio) {
		const t0 = audio.currentTime;
		for (const note of CHIME_NOTES) {
			const start = t0 + note.at;
			const end = start + CHIME_NOTE_SECONDS;
			const osc = audio.createOscillator();
			osc.type = "sine";
			osc.frequency.setValueAtTime(note.hz, start);
			const gain = audio.createGain();
			gain.gain.setValueAtTime(CHIME_FLOOR, start);
			gain.gain.linearRampToValueAtTime(CHIME_VOLUME, start + CHIME_ATTACK);
			gain.gain.exponentialRampToValueAtTime(CHIME_FLOOR, end);
			osc.connect(gain);
			gain.connect(audio.destination);
			osc.start(start);
			osc.stop(end);
		}
	}
};
// src/office/done-mark.ts
/** 끝남 표시가 온전히 보이는 시간(밀리초, 원본 2.0초). */
var DONE_SHOW_MS = 2e3;
/** 담당마다의 끝남 표시. */
var DoneMarks = class {
	/** 담당 → 끝난 시각(밀리초). */
	finished = /* @__PURE__ */ new Map();
	/** 그 담당의 일이 `now`에 끝났다: ✓를 새로 띄운다(이미 있으면 `now`부터 다시 센다). */
	finish(agent, now) {
		this.finished.set(agent, now);
	}
	/** 그 담당이 새 일을 시작했다: 끝남 표시를 바로 지운다. */
	start(agent, now) {
		this.finished.delete(agent);
	}
	/**
	* `now`에 그 담당에게 보이는 끝남 표시. 끝난 뒤 [`DONE_SHOW_MS`] 동안 `opacity` 1, 그 뒤 [`DONE_FADE_MS`] 동안
	* 1에서 0으로 고르게(직선으로) 줄고, 끝난 뒤 `DONE_SHOW_MS + DONE_FADE_MS`부터는 `undefined`.
	*/
	shown(agent, now) {
		const at = this.finished.get(agent);
		if (at === void 0) return void 0;
		const age = Math.max(0, now - at);
		if (age >= 2500) {
			this.finished.delete(agent);
			return;
		}
		const opacity = age < 2e3 ? 1 : 1 - (age - DONE_SHOW_MS) / 500;
		const mark = markOf("done");
		return {
			mark: mark.id,
			text: mark.glyph,
			color: mark.color,
			opacity
		};
	}
	/** 지금 끝남 표시가 걸린 담당들(다 사라진 것은 빠진다). */
	agents(now) {
		return [...this.finished.keys()].filter((agent) => this.shown(agent, now) !== void 0);
	}
};
/** 일하는 중인 일 상태(이것에서 쉼으로 바뀌면 일이 끝난 것이다). */
var BUSY = /* @__PURE__ */ new Set(["queued", "thinking"]);
/** 표시와 이웃 사각형 사이 틈(px). */
var MARK_GAP = 4;
/**
* 표시 자리 고르기(넘긴 순서대로): 머리 위 3×3 후보([`bubbleCandidates`]) 다음에 자기 말풍선의 오른쪽 · 왼쪽 · 위.
* 화면 안이고 `taken` · 앞서 놓은 표시와 겹치지 않는 첫 자리. 없으면 화면 안인 첫 후보(그것도 없으면 첫 후보).
*/
function placeMarks(items, taken, screen) {
	const placed = [];
	for (const item of items) {
		const { width, height } = item.size;
		const candidates = bubbleCandidates(item.head.x, item.head.y, item.size);
		if (item.own) {
			const o = item.own;
			candidates.push({
				x: o.x + o.width + MARK_GAP,
				y: o.y,
				width,
				height
			}, {
				x: o.x - width - MARK_GAP,
				y: o.y,
				width,
				height
			}, {
				x: o.x + (o.width - width) / 2,
				y: o.y - height - MARK_GAP,
				width,
				height
			});
		}
		const free = (r) => insideScreen(r, screen) && !taken.some((t) => overlap(t, r)) && !placed.some((p) => overlap(p.rect, r));
		const rect = candidates.find(free) ?? candidates.find((r) => insideScreen(r, screen)) ?? candidates[0];
		placed.push({
			agent: item.agent,
			rect
		});
	}
	return placed;
}
function makeAudio() {
	const Audio = window.AudioContext ?? window.webkitAudioContext;
	if (!Audio) throw new Error("no audio");
	return new Audio();
}
/** 표시들을 만들어 잇는다. */
function createOfficeMarks(options) {
	const board = new ApprovalBubbles();
	const done = new DoneMarks();
	const chime = new Chime(makeAudio, soundEnabled);
	/** 이벤트로 끝난 번호(승인은 다시 열리지 않는다 — 늦게 온 다시 읽기가 되살리지 않게). */
	const closed = /* @__PURE__ */ new Set();
	const listeners = [];
	const changed = () => {
		for (const listener of listeners) listener();
	};
	const nameOf = (agent) => options.names[agent] ?? agent;
	const unlock = () => {
		chime.unlock();
	};
	options.gestures.addEventListener("pointerdown", unlock, { once: true });
	options.gestures.addEventListener("keydown", unlock, { once: true });
	renderLegend(options.legendRoot);
	function open(approval) {
		const root = options.approvals;
		applyApproval(root, {
			number: approval.number,
			summary: approval.summary,
			short_code: approval.short_code,
			status: "pending"
		}, options.fetcher);
		const card = [...root.querySelectorAll("[data-approval]")].find((c) => c.dataset.approval === approval.number);
		if (!card) return;
		card.tabIndex = -1;
		card.dataset.focus = "1";
		card.scrollIntoView({ block: "nearest" });
		card.focus({ preventScroll: true });
		window.setTimeout(() => {
			delete card.dataset.focus;
		}, 1600);
	}
	function press(agent) {
		board.press(agent, open);
	}
	/** 기다리지 않는 카드는 닫는다(이벤트를 놓쳐도 끝난 카드가 남지 않게). */
	function closeStaleCards(waiting) {
		for (const card of options.approvals.querySelectorAll("[data-approval]")) {
			const number = card.dataset.approval ?? "";
			if (!waiting.has(number) && card.dataset.deciding !== "1") card.remove();
		}
	}
	function handOf(agent, now) {
		return board.bubble(agent, now);
	}
	function handButton(hand, agent) {
		const button = document.createElement("button");
		button.type = "button";
		button.className = "mark-hand";
		button.dataset.mark = "approval";
		button.style.color = hand.color;
		button.addEventListener("click", (event) => {
			event.stopPropagation();
			press(agent);
		});
		paintHand(button, hand, agent);
		return button;
	}
	function paintHand(button, hand, agent) {
		if (button.textContent !== hand.text) button.textContent = hand.text;
		const label = `${nameOf(agent)} · 승인 기다림 ${hand.count}건 — 승인 카드 열기`;
		if (button.getAttribute("aria-label") !== label) {
			button.setAttribute("aria-label", label);
			button.title = label;
		}
	}
	function doneSpan(shown) {
		const span = document.createElement("span");
		span.className = "mark-done";
		span.dataset.mark = "done";
		span.textContent = shown.text;
		span.style.color = shown.color;
		span.style.opacity = String(shown.opacity);
		span.setAttribute("role", "img");
		span.setAttribute("aria-label", markOf("done").label);
		return span;
	}
	function layer(stage) {
		const host = document.createElement("div");
		host.className = "overhead-marks";
		stage.appendChild(host);
		const shown = /* @__PURE__ */ new Map();
		function element(agent) {
			let one = shown.get(agent);
			if (!one) {
				const el = document.createElement("div");
				el.className = "overhead-mark";
				el.dataset.agent = agent;
				host.appendChild(el);
				one = {
					el,
					hand: null,
					done: null
				};
				shown.set(agent, one);
			}
			return one;
		}
		/** 이번 프레임의 표시로 고친다. 크기가 바뀌었으면 참. */
		function refresh(agent, hand, check) {
			const one = element(agent);
			let resized = false;
			if (hand && !one.hand) {
				one.hand = handButton(hand, agent);
				one.el.prepend(one.hand);
				resized = true;
			} else if (!hand && one.hand) {
				one.hand.remove();
				one.hand = null;
				resized = true;
			} else if (hand && one.hand) {
				const before = one.hand.textContent;
				paintHand(one.hand, hand, agent);
				resized ||= before !== one.hand.textContent;
			}
			if (check && !one.done) {
				one.done = doneSpan(check);
				one.el.append(one.done);
				resized = true;
			} else if (!check && one.done) {
				one.done.remove();
				one.done = null;
				resized = true;
			} else if (check && one.done) one.done.style.opacity = String(check.opacity);
			one.el.style.display = "";
			return resized;
		}
		const sizes = /* @__PURE__ */ new Map();
		return {
			place(now, spots, taken, own, screen) {
				const items = [];
				const drawn = /* @__PURE__ */ new Set();
				for (const spot of spots) {
					const hand = handOf(spot.id, now);
					const check = done.shown(spot.id, now);
					if (!spot.visible || !hand && !check) continue;
					const resized = refresh(spot.id, hand, check);
					let size = sizes.get(spot.id);
					if (!size || resized) {
						const box = element(spot.id).el.getBoundingClientRect();
						size = {
							width: box.width || 28,
							height: box.height || 26
						};
						sizes.set(spot.id, size);
					}
					drawn.add(spot.id);
					items.push({
						agent: spot.id,
						head: spot.head,
						size,
						own: own.get(spot.id)
					});
				}
				for (const { agent, rect } of placeMarks(items, taken, screen)) element(agent).el.style.transform = `translate(${Math.round(rect.x)}px, ${Math.round(rect.y)}px)`;
				for (const [agent, one] of shown) if (!drawn.has(agent)) {
					one.el.style.display = "none";
					if (one.done) {
						one.done.remove();
						one.done = null;
						sizes.delete(agent);
					}
				}
			},
			dispose() {
				host.remove();
				shown.clear();
			}
		};
	}
	return {
		onApproval(payload) {
			const outcome = board.apply(payload, Date.now());
			const number = payload?.number;
			if (typeof number !== "string") return;
			if (outcome === "new") chime.ring(number);
			if (outcome === "closed") closed.add(number);
			if (outcome !== "ignored") changed();
		},
		sync(list, ringNew) {
			if (!Array.isArray(list)) return;
			const before = new Set(board.waiting().map((w) => w.number));
			const live = list.filter((row) => !closed.has(String(row?.number ?? "")));
			board.sync(live);
			const now = board.waiting().map((w) => w.number);
			if (ringNew) {
				for (const number of now) if (!before.has(number)) chime.ring(number);
			} else chime.quiet(now);
			closeStaleCards(new Set(now));
			changed();
		},
		work(agent, before, after, now) {
			if (BUSY.has(after)) {
				if (done.shown(agent, now)) {
					done.start(agent, now);
					changed();
				}
				return;
			}
			if (after === "idle" && BUSY.has(before)) {
				done.finish(agent, now);
				changed();
				window.setTimeout(changed, DONE_SHOW_MS + 500 + 50);
			}
		},
		layer,
		badges(agent, now) {
			const out = [];
			const hand = handOf(agent, now);
			if (hand) out.push(handButton(hand, agent));
			const check = done.shown(agent, now);
			if (check) out.push(doneSpan(check));
			return out;
		},
		onChange(listener) {
			listeners.push(listener);
		}
	};
}
var FACINGS = [
	"up",
	"down",
	"left",
	"right"
];
function isCell(value) {
	return typeof value === "number" && Number.isInteger(value) && value >= 0;
}
/** 내 캐릭터를 읽는다([`API.officeMe`]). 응답이 실패이거나 모양이 틀리면 `null`(던지지 않는다). */
async function loadMe(fetcher) {
	try {
		const reply = await fetcher(API.officeMe, { credentials: "same-origin" });
		if (!reply.ok) return null;
		const body = await reply.json();
		if (!body || typeof body !== "object") return null;
		const { name, look, x, y, dir } = body;
		if (typeof name !== "string" || typeof look !== "string") return null;
		if (!isCell(x) || !isCell(y)) return null;
		if (typeof dir !== "string" || !FACINGS.includes(dir)) return null;
		return {
			name,
			look,
			x,
			y,
			dir
		};
	} catch {
		return null;
	}
}
/**
* 자리 하나를 보낸다: `POST` [`API.officeMePosition`] 몸통 `{"x", "y", "dir"}`(JSON, 같은 출처 · 떠날 때도 닿게
* `keepalive`). 실패해도 던지지 않고 `false`를 돌려준다.
*/
async function postSpot(fetcher, spot) {
	try {
		return (await fetcher(API.officeMePosition, {
			method: "POST",
			credentials: "same-origin",
			keepalive: true,
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				x: spot.x,
				y: spot.y,
				dir: spot.dir
			})
		})).ok;
	} catch {
		return false;
	}
}
function same(a, b) {
	return a !== null && b !== null && a.x === b.x && a.y === b.y && a.dir === b.dir;
}
/**
* 띄엄띄엄 보내는 것. `send`는 자리를 보내는 함수(보통 `postSpot`)이고 실패(거부 · 던짐)해도 이것은 던지지 않는다.
* - `moved(spot, now)`: 지금 자리. 마지막으로 보낸 자리와 다르고 마지막으로 보낸 지 [`SAVE_INTERVAL_MS`]가 지났으면 보낸다.
* - `tick(now)`: 프레임마다 부른다. 보내지 않은 자리가 있고 간격이 지났으면 보낸다(멈춘 뒤 마지막 자리).
* - `flush()`: 화면을 떠날 때. 보내지 않은 자리가 있으면 간격과 상관없이 곧바로 보낸다.
* 같은 자리(칸 · 방향이 같음)는 다시 보내지 않는다. 처음 만들 때 `initial`(서버에서 읽은 자리)을 보낸 것으로 본다.
*/
var SpotSender = class {
	send;
	sent;
	sentAt = -Infinity;
	pending = null;
	constructor(send, initial = null) {
		this.send = send;
		this.sent = initial ? { ...initial } : null;
	}
	moved(spot, now) {
		this.pending = same(spot, this.sent) ? null : {
			x: spot.x,
			y: spot.y,
			dir: spot.dir
		};
		this.tick(now);
	}
	tick(now) {
		if (this.pending === null || now - this.sentAt < 1e3) return;
		this.sentAt = now;
		this.push();
	}
	flush() {
		if (this.pending === null) return;
		this.push();
	}
	push() {
		const spot = this.pending;
		if (spot === null) return;
		this.pending = null;
		this.sent = spot;
		try {
			Promise.resolve(this.send(spot)).catch(() => void 0);
		} catch {}
	}
};
// src/office/mention-text.ts
/** 멘션 없는 팀 방 글에 보이는 안내(E44 · 4b2). */
var NO_MENTION_HINT = "@로 담당을 부르거나 공지로 보내세요";
/** 멘션한 이름이 명단에 없을 때 보낸 사람에게만 보이는 안내(E43 · 4d1, 원본 글 그대로). */
var NO_MATCH_HINT = "지목한 상대가 이 방에 없어 아무도 답하지 않았습니다.";
/** `@` 목록에 맞는 담당이 없을 때의 줄. */
var MENTION_EMPTY = "해당하는 담당이 없습니다";
/** 이름 → 보낼 글의 멘션 `@[이름]`(이름 속 `\` `[` `]` 앞에 `\`). */
function mentionTag(name) {
	let tag = "@[";
	for (const c of name) {
		if (c === "\\" || c === "[" || c === "]") tag += "\\";
		tag += c;
	}
	return `${tag}]`;
}
/** 조각들 → 서버로 보낼 글. 글 조각은 그대로, 칩은 [`mentionTag`]로 잇는다(다듬지 않는다). */
function serializePieces(pieces) {
	return pieces.map((p) => p.kind === "text" ? p.text : mentionTag(p.name)).join("");
}
/** 조각에 멘션 칩이 하나라도 있는가. */
function hasMention(pieces) {
	return pieces.some((p) => p.kind === "mention");
}
/** 빈칸(줄바꿈 포함)인가. */
function isSpace(c) {
	return /\s/.test(c);
}
/**
* 글 `text`의 커서 `caret`(글자 위치) 앞에서 열린 `@` 찾기. `@`가 줄 처음이거나 빈칸(줄바꿈 포함) 바로 뒤이고,
* `@`부터 커서까지에 빈칸이 없을 때만 `{ start: "@"의 위치, query: "@" 뒤부터 커서까지 }`. 아니면 `null`.
*/
function mentionQuery(text, caret) {
	const before = text.slice(0, Math.max(0, Math.min(caret, text.length)));
	for (let i = before.length - 1; i >= 0; i -= 1) {
		const c = before[i];
		if (isSpace(c)) return null;
		if (c === "@") {
			if (i > 0 && !isSpace(before[i - 1])) return null;
			return {
				start: i,
				query: before.slice(i + 1)
			};
		}
	}
	return null;
}
/** 찾는 말로 명단을 거른다: 소문자로 바꾼 이름에 소문자로 바꾼 찾는 말이 들어 있는 담당(명단 순서). 빈 말은 모두. */
function filterMembers(members, query) {
	const want = query.toLowerCase();
	return members.filter((m) => m.name.toLowerCase().includes(want));
}
/** 목록 이동: `count`개 가운데 `index`에서 `delta`(±1)만큼, 끝에서 처음으로 · 처음에서 끝으로 돈다. `count`가 0이면 0. */
function cycle(index, count, delta) {
	if (count <= 0) return 0;
	return ((index + delta) % count + count) % count;
}
/** 서버 답의 `hint` → 보낸 사람에게 보일 안내. `"no_mention"` → [`NO_MENTION_HINT`], `"no_match"` → [`NO_MATCH_HINT`], 그 밖은 `null`. */
function hintText(hint) {
	if (hint === "no_mention") return NO_MENTION_HINT;
	if (hint === "no_match") return NO_MATCH_HINT;
	return null;
}
var ACTOR_RADIUS = .22;
/** 지도 정의에서 걸을 수 없는 칸의 표를 만든다. */
function buildGrid(map) {
	const cols = map.cols;
	const rows = map.rows;
	const blocked = new Uint8Array(cols * rows);
	const mark = (r) => {
		for (let y = r.y; y < r.y + r.h; y++) for (let x = r.x; x < r.x + r.w; x++) if (x >= 0 && y >= 0 && x < cols && y < rows) blocked[y * cols + x] = 1;
	};
	for (const w of map.walls || []) mark(w);
	for (const f of map.furniture || []) if (f.blocks) mark(f);
	return {
		cols,
		rows,
		walkable(x, y) {
			return x >= 0 && y >= 0 && x < cols && y < rows && blocked[y * cols + x] === 0;
		}
	};
}
var Heap = class {
	items = [];
	get size() {
		return this.items.length;
	}
	push(node) {
		const a = this.items;
		a.push(node);
		let i = a.length - 1;
		while (i > 0) {
			const p = i - 1 >> 1;
			if (a[p].f <= a[i].f) break;
			[a[p], a[i]] = [a[i], a[p]];
			i = p;
		}
	}
	pop() {
		const a = this.items;
		const top = a[0];
		const last = a.pop();
		if (a.length > 0) {
			a[0] = last;
			let i = 0;
			for (;;) {
				const l = i * 2 + 1;
				const r = l + 1;
				let m = i;
				if (l < a.length && a[l].f < a[m].f) m = l;
				if (r < a.length && a[r].f < a[m].f) m = r;
				if (m === i) break;
				[a[m], a[i]] = [a[i], a[m]];
				i = m;
			}
		}
		return top;
	}
};
var STEPS = [
	[
		1,
		0,
		1
	],
	[
		-1,
		0,
		1
	],
	[
		0,
		1,
		1
	],
	[
		0,
		-1,
		1
	],
	[
		1,
		1,
		Math.SQRT2
	],
	[
		1,
		-1,
		Math.SQRT2
	],
	[
		-1,
		1,
		Math.SQRT2
	],
	[
		-1,
		-1,
		Math.SQRT2
	]
];
/**
* 칸 `from`에서 칸 `to`로 가는 경로(칸 중심 좌표 목록, 실수)를 찾는다. 못 찾으면 null.
* 출발 칸은 언제나 걸을 수 있는 것으로 친다. 도착 칸이 막혔으면 실패.
*/
function findPath(grid, from, to) {
	if (!grid.walkable(to.x, to.y)) return null;
	const key = (x, y) => y * grid.cols + x;
	const h = (x, y) => Math.hypot(to.x - x, to.y - y);
	const open = new Heap();
	const best = /* @__PURE__ */ new Map();
	const parent = /* @__PURE__ */ new Map();
	const start = key(from.x, from.y);
	best.set(start, 0);
	open.push({
		x: from.x,
		y: from.y,
		g: 0,
		f: h(from.x, from.y)
	});
	let expanded = 0;
	while (open.size > 0) {
		const cur = open.pop();
		const ck = key(cur.x, cur.y);
		if (cur.g > (best.get(ck) ?? Infinity)) continue;
		if (cur.x === to.x && cur.y === to.y) {
			const cells = [];
			let k = ck;
			while (k !== void 0) {
				cells.push({
					x: k % grid.cols,
					y: Math.floor(k / grid.cols)
				});
				k = parent.get(k);
			}
			cells.reverse();
			return smooth(grid, cells.map((c) => ({
				x: c.x + .5,
				y: c.y + .5
			})));
		}
		if (++expanded > 4096) return null;
		for (const [dx, dy, cost] of STEPS) {
			const nx = cur.x + dx;
			const ny = cur.y + dy;
			if (!grid.walkable(nx, ny)) continue;
			if (dx !== 0 && dy !== 0 && !(grid.walkable(cur.x + dx, cur.y) && grid.walkable(cur.x, cur.y + dy))) continue;
			const nk = key(nx, ny);
			const g = cur.g + cost;
			if (g < (best.get(nk) ?? Infinity)) {
				best.set(nk, g);
				parent.set(nk, ck);
				open.push({
					x: nx,
					y: ny,
					g,
					f: g + h(nx, ny)
				});
			}
		}
	}
	return null;
}
/** 두 점 사이를 반지름 0.22칸의 몸이 벽에 닿지 않고 곧게 갈 수 있는가. */
function clearLine(grid, a, b) {
	const dist = Math.hypot(b.x - a.x, b.y - a.y);
	const steps = Math.max(1, Math.ceil(dist / .1));
	const r = ACTOR_RADIUS;
	for (let i = 0; i <= steps; i++) {
		const t = i / steps;
		const x = a.x + (b.x - a.x) * t;
		const y = a.y + (b.y - a.y) * t;
		for (const [ox, oy] of [
			[-.22, -.22],
			[r, -.22],
			[-.22, r],
			[r, r]
		]) if (!grid.walkable(Math.floor(x + ox), Math.floor(y + oy))) return false;
	}
	return true;
}
/** 곧게 갈 수 있는 가장 먼 점으로 건너뛰어 꺾이는 점만 남긴다. */
function smooth(grid, points) {
	if (points.length <= 2) return points;
	const out = [points[0]];
	let i = 0;
	while (i < points.length - 1) {
		let j = points.length - 1;
		while (j > i + 1 && !clearLine(grid, points[i], points[j])) j--;
		out.push(points[j]);
		i = j;
	}
	return out;
}
// src/office/pathfind.ts
/** `grid`에 `cells`(담당이 선 칸)를 막힌 칸으로 더한 새 표. `grid`는 바꾸지 않는다. */
function withBlockers(grid, cells) {
	const taken = new Set(cells.map((c) => `${c.x},${c.y}`));
	return {
		cols: grid.cols,
		rows: grid.rows,
		walkable(x, y) {
			return grid.walkable(x, y) && !taken.has(`${x},${y}`);
		}
	};
}
/** 실수 자리 → 그 칸(정수, 내림). */
function cellOf(pos) {
	return {
		x: Math.floor(pos.x),
		y: Math.floor(pos.y)
	};
}
/**
* 칸 `from`에서 칸 `to`까지의 길(칸 가운데 점, 마지막은 `to`의 가운데). 막힌 칸을 피해 돌아가고, 길이 없거나
* `to`가 막혔으면 `null`(움직이지 않는다 — E40). 같은 칸이면 빈 길.
*/
function pathTo(grid, from, to) {
	if (!grid.walkable(to.x, to.y)) return null;
	if (from.x === to.x && from.y === to.y) return [];
	const points = findPath(grid, from, to);
	if (points === null) return null;
	return points.slice(1);
}
/** 길의 길이(칸): 출발 칸 가운데에서 점들을 이은 거리. */
function lengthOf(from, path) {
	let total = 0;
	let here = {
		x: from.x + .5,
		y: from.y + .5
	};
	for (const p of path) {
		total += Math.hypot(p.x - here.x, p.y - here.y);
		here = p;
	}
	return total;
}
/** 네 이웃(위 · 아래 · 왼쪽 · 오른쪽). */
var NEIGHBORS = [
	{
		x: 0,
		y: -1
	},
	{
		x: 0,
		y: 1
	},
	{
		x: -1,
		y: 0
	},
	{
		x: 1,
		y: 0
	}
];
/**
* 담당(칸 `agent`) 옆 빈 칸: 네 이웃(위 · 아래 · 왼쪽 · 오른쪽) 가운데 걸을 수 있고 `from`에서 길이 있는 칸 중
* 길이 가장 짧은 칸. 없으면 `null`.
*/
function besideCell(grid, from, agent) {
	let best = null;
	for (const d of NEIGHBORS) {
		const cell = {
			x: agent.x + d.x,
			y: agent.y + d.y
		};
		if (!grid.walkable(cell.x, cell.y)) continue;
		const path = pathTo(grid, from, cell);
		if (path === null) continue;
		const length = lengthOf(from, path);
		if (best === null || length < best.length) best = {
			cell,
			length
		};
	}
	return best === null ? null : best.cell;
}
/** 입구 칸: 입구 가운데 열, 입구 줄 바로 안쪽(한 줄 위). 지도에 입구가 없으면 지도 아래 가운데의 한 줄 위. */
function entranceCell(map) {
	const e = map.entrance;
	if (e) return {
		x: Math.floor((e.fromCol + e.toCol) / 2),
		y: e.row - 1
	};
	return {
		x: Math.floor(map.cols / 2),
		y: map.rows - 2
	};
}
/**
* 처음 서는 칸(M8): 저장된 칸이 걸을 수 있으면 그 칸, 아니면(없거나 막혔으면) 입구, 입구도 막혔으면 입구에서
* 가장 가까운 걸을 수 있는 칸.
*/
function startCell(grid, saved, entrance) {
	if (saved && grid.walkable(saved.x, saved.y)) return {
		x: saved.x,
		y: saved.y
	};
	if (grid.walkable(entrance.x, entrance.y)) return {
		x: entrance.x,
		y: entrance.y
	};
	const reach = grid.cols + grid.rows;
	for (let ring = 1; ring <= reach; ring += 1) {
		let best = null;
		let bestDist = Infinity;
		for (let dx = -ring; dx <= ring; dx += 1) {
			const rest = ring - Math.abs(dx);
			for (const dy of rest === 0 ? [0] : [-rest, rest]) {
				const x = entrance.x + dx;
				const y = entrance.y + dy;
				if (!grid.walkable(x, y)) continue;
				const dist = Math.hypot(dx, dy);
				if (dist < bestDist) {
					best = {
						x,
						y
					};
					bestDist = dist;
				}
			}
		}
		if (best) return best;
	}
	return {
		x: entrance.x,
		y: entrance.y
	};
}
// src/office/summon-client.ts
/** JSON을 같은 출처로 `POST`한다. 닿지 못하면 `null`(던지지 않는다). */
async function postJson(fetcher, url, body) {
	try {
		return await fetcher(url, {
			method: "POST",
			credentials: "same-origin",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body)
		});
	} catch {
		return null;
	}
}
/** 담당을 부른다. 200이면 `called`, 409 `meeting_reserved`면 `meeting`, 400 `unknown_agent`면 `unknown`, 그 밖은 `failed`. */
async function requestSummon(fetcher, agent) {
	const reply = await postJson(fetcher, API.officeSummon, { agent });
	if (!reply) return { kind: "failed" };
	if (reply.ok) return { kind: "called" };
	const body = await reply.json().catch(() => null);
	if (reply.status === 409 && body?.error === "meeting_reserved") return { kind: "meeting" };
	if (reply.status === 400 && body?.error === "unknown_agent") return { kind: "unknown" };
	return { kind: "failed" };
}
/** 도착을 알린다. 200이면 참. */
async function reportArrived(fetcher, agent) {
	return (await postJson(fetcher, API.officeSummonArrived, { agent }))?.ok === true;
}
/** 지금 보는 방을 알린다. 200이면 참. */
async function reportView(fetcher, room) {
	return (await postJson(fetcher, API.officeSummonView, { room }))?.ok === true;
}
// src/office/summon-view.ts
/** 부를 때 걷기 빠르기(칸/초, 원본 300px/s ÷ 32px). */
var SUMMON_SPEED = 9.4;
/** 돌아갈 때 걷기 빠르기(칸/초, 원본 150px/s). */
var RETURN_SPEED = 4.7;
/** 도착으로 보는 거리(칸, 원본 36px). */
var ARRIVE_RADIUS = 1.125;
/** 담당 목록의 부르기 단추 글. */
var SUMMON_BUTTON = "부르기";
/** 부르기 단추의 class. */
var SUMMON_BUTTON_CLASS = "summon";
var PHASES = [
	"idle",
	"called",
	"waiting",
	"returning"
];
var REASONS = ["call", "mention"];
/** `office.summon` 내용 → 이벤트. 모양이 틀리거나 모르는 단계 · 까닭이면 `null`. */
function parseSummon(payload) {
	if (typeof payload !== "object" || payload === null) return null;
	const { agent, phase, reason, at } = payload;
	if (typeof agent !== "string" || typeof phase !== "string" || !PHASES.includes(phase)) return null;
	if (reason !== null && reason !== void 0 && !(typeof reason === "string" && REASONS.includes(reason))) return null;
	return {
		agent,
		phase,
		reason: reason ?? null,
		at: typeof at === "number" ? at : 0
	};
}
/**
* 단계 `phase`에서 칸 `agent`의 담당을 어떻게 그릴지. `player`는 내 칸, `home`은 그 담당의 자리 칸.
* - `called`: `agent`가 `player`에서 [`ARRIVE_RADIUS`] 안이면 `arrived`, 아니면 `besideCell(grid, agent, player)`로
*   `walk`(빠르기 [`SUMMON_SPEED`]), 곁 칸이 없으면 `stay`.
* - `waiting`: `face`(나를 보는 쪽, [`facingToward`]).
* - `returning`: `home`으로 `walk`(빠르기 [`RETURN_SPEED`]).
* - `idle`: `home`.
*/
function summonDraw(phase, grid, agent, player, home) {
	switch (phase) {
		case "called": {
			if (arrivedNear(agent, player)) return { kind: "arrived" };
			const to = besideCell(grid, agent, player);
			return to ? {
				kind: "walk",
				to,
				speed: SUMMON_SPEED
			} : { kind: "stay" };
		}
		case "waiting": return {
			kind: "face",
			facing: facingToward(agent, player)
		};
		case "returning": return {
			kind: "walk",
			to: home,
			speed: RETURN_SPEED
		};
		default: return { kind: "home" };
	}
}
/** `from`에서 `to`를 보는 네 방향: 가로 거리가 세로 거리보다 크면 left/right, 아니면 up/down(y가 크면 down). */
function facingToward(from, to) {
	const dx = to.x - from.x;
	const dy = to.y - from.y;
	if (Math.abs(dx) > Math.abs(dy)) return dx > 0 ? "right" : "left";
	return dy > 0 ? "down" : "up";
}
/** 걷는 담당 `pos`가 목표 `target`에 닿았는가(거리 [`ARRIVE_RADIUS`] 이하). */
function arrivedNear(pos, target) {
	return Math.hypot(target.x - pos.x, target.y - pos.y) <= ARRIVE_RADIUS;
}
/** 도착했을 때 1:1 대화를 여는가: 부름(`call`)이면 참, 멘션으로 왔으면 거짓(4a3 · 4c4). */
function opensConversation(reason) {
	return reason === "call";
}
/** 회의 중 안내(E47): `{이름}{은/는} 지금 회의 중이에요`(받침 있으면 "은", 없으면 "는", 한글이 아니면 "은(는)"). */
function meetingNotice(name) {
	const last = name.charCodeAt(name.length - 1);
	let particle = "은(는)";
	if (last >= 44032 && last <= 55203) particle = (last - 44032) % 28 === 0 ? "는" : "은";
	return `${name}${particle} 지금 회의 중이에요`;
}
/**
* 대화 바꾸기 `prev` → `next`(대화 키: 팀 방 `"team"` 또는 담당 id)에서 서버에 알릴 방. 팀 방 ↔ 1:1 방이 바뀔 때만
* `"team"` · `"direct"`, 같은 쪽 안에서 바뀌면(1:1 → 다른 1:1 포함) `null`.
*/
function viewChange(prev, next) {
	const wasTeam = prev === "team";
	const isTeam = next === "team";
	if (wasTeam === isTeam) return null;
	return isTeam ? "team" : "direct";
}
/** 담당 목록 줄에 다는 부르기 단추(글 [`SUMMON_BUTTON`], class [`SUMMON_BUTTON_CLASS`], `data-agent`). 누르면 `onSummon(agent)`. */
function summonButton(agent, onSummon) {
	const button = document.createElement("button");
	button.type = "button";
	button.className = SUMMON_BUTTON_CLASS;
	button.dataset.agent = agent;
	button.textContent = SUMMON_BUTTON;
	button.addEventListener("click", (event) => {
		event.stopPropagation();
		onSummon(agent);
	});
	return button;
}
// src/office/summon-scene.ts
/** 보는 쪽 → y축 회전(가구 · 담당과 같은 값). */
var YAW = {
	down: 0,
	up: Math.PI,
	left: -Math.PI / 2,
	right: Math.PI / 2
};
/** 다시 들어왔을 때 곁에 있다고 보는 거리(칸). */
var NEAR = 2;
function createSummonScene(options) {
	const grid = buildGrid(options.map);
	const phases = /* @__PURE__ */ new Map();
	const arrived = (agent, reason, opens) => {
		reportArrived(options.fetcher, agent);
		if (opens && opensConversation(reason)) options.openChat(agent);
	};
	function steerTo(view, agent, to, yaw, onArrive) {
		const steer = {
			to,
			speed: SUMMON_SPEED,
			yaw,
			onArrive
		};
		view.steerAgent(agent, steer);
	}
	function draw(agent) {
		const event = phases.get(agent);
		if (!event) return;
		const view = options.view();
		const here = view?.agentAt(agent) ?? null;
		const me = view?.playerAt() ?? null;
		if (!view || !here || !me) {
			if (event.phase === "called" || event.phase === "returning") arrived(agent, event.reason, event.phase === "called");
			return;
		}
		const draw = summonDraw(event.phase, grid, cellOf(here), cellOf(me), cellOf(here));
		const toward = YAW[facingToward(here, me)];
		switch (draw.kind) {
			case "walk":
				if (event.phase === "returning") view.steerAgent(agent, {
					to: null,
					speed: draw.speed,
					onArrive: () => arrived(agent, event.reason, false)
				});
				else steerTo(view, agent, draw.to, YAW[facingToward(draw.to, cellOf(me))], () => arrived(agent, event.reason, true));
				return;
			case "arrived":
			case "stay":
				steerTo(view, agent, cellOf(here), toward);
				arrived(agent, event.reason, true);
				return;
			case "face":
				faceOrCome(view, agent, here, me);
				return;
			default: view.steerAgent(agent, null);
		}
	}
	/** 곁에서 기다림: 곁이면 나를 보고, 멀면(다시 들어옴) 곁으로 걸어온다. */
	function faceOrCome(view, agent, here, me) {
		if (Math.hypot(here.x - me.x, here.y - me.y) <= NEAR || arrivedNear(here, me)) {
			steerTo(view, agent, cellOf(here), YAW[facingToward(here, me)]);
			return;
		}
		const come = summonDraw("called", grid, cellOf(here), cellOf(me), cellOf(here));
		if (come.kind === "walk") steerTo(view, agent, come.to, YAW[facingToward(come.to, cellOf(me))]);
	}
	return {
		onEvent(payload) {
			const event = parseSummon(payload);
			if (!event) return;
			phases.set(event.agent, event);
			draw(event.agent);
		},
		fromSnapshot(agent, phase, reason) {
			const event = parseSummon({
				agent,
				phase,
				reason: reason ?? null,
				at: 0
			});
			if (!event || event.phase === "idle") return;
			phases.set(agent, event);
		},
		redraw() {
			for (const agent of phases.keys()) draw(agent);
		},
		async summon(agent) {
			const result = await requestSummon(options.fetcher, agent);
			if (result.kind === "meeting") options.say(meetingNotice(options.nameOf(agent)));
			else if (result.kind !== "called") options.say(`${options.nameOf(agent)}을(를) 부르지 못했습니다`);
		},
		conversation(prev, next) {
			const room = viewChange(prev, next);
			if (room) reportView(options.fetcher, room);
		}
	};
}
// src/office/mention-editor.ts
/** 칩의 class. */
var CHIP_CLASS = "mention-chip";
/** 목록의 class(닫히면 `hidden`). */
var LIST_CLASS = "mention-list";
/** 목록 줄의 class(고른 줄은 `active`도 붙고 `aria-selected="true"`). */
var OPTION_CLASS = "mention-option";
/** 칩 하나를 만든다(보이는 글 `@이름`, 통째로 지워지게 편집 불가). */
function makeChip(member) {
	const chip = document.createElement("span");
	chip.className = CHIP_CLASS;
	chip.setAttribute("contenteditable", "false");
	chip.dataset.agent = member.agent;
	chip.dataset.name = member.name;
	chip.textContent = `@${member.name}`;
	return chip;
}
function isChip(node) {
	return node instanceof HTMLElement && node.classList.contains("mention-chip");
}
/** 글 칸의 조각(이웃한 글은 합치고 빈 글은 뺀다). 줄바꿈 요소는 `\n`이다. */
function readPieces(input) {
	const pieces = [];
	const pushText = (text) => {
		if (text === "") return;
		const last = pieces[pieces.length - 1];
		if (last && last.kind === "text") last.text += text;
		else pieces.push({
			kind: "text",
			text
		});
	};
	input.childNodes.forEach((node) => {
		if (isChip(node)) pieces.push({
			kind: "mention",
			agent: node.dataset.agent ?? "",
			name: node.dataset.name ?? ""
		});
		else if (node instanceof HTMLBRElement) pushText("\n");
		else pushText(node.textContent ?? "");
	});
	return pieces;
}
/** 커서가 든 글 칸 안의 자리(글 칸 밖이면 끝). */
function caretOf(input) {
	const selection = document.getSelection();
	const node = selection?.anchorNode ?? null;
	if (selection && node && input.contains(node)) return {
		node,
		offset: selection.anchorOffset
	};
	const last = input.lastChild;
	if (last instanceof Text) return {
		node: last,
		offset: last.data.length
	};
	return {
		node: input,
		offset: input.childNodes.length
	};
}
/** 커서를 그 자리에 둔다. */
function placeCaret(node, offset) {
	const selection = document.getSelection();
	if (!selection) return;
	const range = document.createRange();
	range.setStart(node, offset);
	range.collapse(true);
	selection.removeAllRanges();
	selection.addRange(range);
}
/** 커서 바로 앞이 칩이면 그 칩. */
function chipBeforeCaret(input) {
	const { node, offset } = caretOf(input);
	if (node instanceof Text) return offset === 0 && isChip(node.previousSibling) ? node.previousSibling : null;
	if (node === input && offset > 0) {
		const before = input.childNodes[offset - 1];
		return isChip(before) ? before : null;
	}
	return null;
}
/** 조합 중인 키인가(한글 입력기). */
function composingKey(event, composing) {
	return composing || event.isComposing || event.keyCode === 229;
}
/** `host` 안에 글 칸과 목록을 만들어 단다. */
function mountMentionEditor(host, options) {
	const input = document.createElement("div");
	input.className = "mention-input";
	input.setAttribute("contenteditable", "true");
	input.setAttribute("role", "textbox");
	const list = document.createElement("div");
	list.className = LIST_CLASS;
	list.setAttribute("role", "listbox");
	list.hidden = true;
	host.append(list, input);
	let spot = null;
	let found = [];
	let active = -1;
	let composing = false;
	const close = () => {
		spot = null;
		found = [];
		active = -1;
		list.hidden = true;
		list.replaceChildren();
	};
	const paint = () => {
		list.replaceChildren();
		if (found.length === 0) {
			const empty = document.createElement("div");
			empty.className = "mention-empty";
			empty.textContent = MENTION_EMPTY;
			list.append(empty);
		}
		found.forEach((member, i) => {
			const option = document.createElement("div");
			option.className = OPTION_CLASS;
			option.setAttribute("role", "option");
			option.textContent = `@${member.name}`;
			const on = i === active;
			option.classList.toggle("active", on);
			option.setAttribute("aria-selected", on ? "true" : "false");
			option.addEventListener("mousedown", (event) => {
				event.preventDefault();
				choose(i);
			});
			list.append(option);
		});
		list.hidden = false;
	};
	const look = () => {
		const { node, offset } = caretOf(input);
		const query = node instanceof Text ? mentionQuery(node.data, offset) : null;
		if (!query || !(node instanceof Text)) {
			close();
			return;
		}
		spot = {
			node,
			start: query.start,
			end: offset
		};
		found = filterMembers(options.members(), query.query);
		active = found.length > 0 ? 0 : -1;
		paint();
	};
	const choose = (index) => {
		const member = found[index];
		if (!spot || !member) return;
		const { node, start, end } = spot;
		const before = node.data.slice(0, start);
		const after = document.createTextNode(` ${node.data.slice(end)}`);
		const chip = makeChip(member);
		if (before === "") node.replaceWith(chip, after);
		else node.replaceWith(document.createTextNode(before), chip, after);
		placeCaret(after, 1);
		close();
	};
	const submit = () => {
		const pieces = readPieces(input);
		const text = serializePieces(pieces);
		if (text.trim() === "") return;
		options.onSubmit(text, pieces);
	};
	const openKey = (event) => {
		if (event.key === "ArrowDown" || event.key === "ArrowUp") {
			event.preventDefault();
			active = cycle(Math.max(active, 0), found.length, event.key === "ArrowDown" ? 1 : -1);
			paint();
		} else if (event.key === "Enter" || event.key === "Tab") {
			event.preventDefault();
			if (active >= 0) choose(active);
		} else if (event.key === "Escape") {
			event.preventDefault();
			close();
		}
	};
	input.addEventListener("keydown", (event) => {
		if (composingKey(event, composing)) return;
		if (spot) {
			openKey(event);
			return;
		}
		if (event.key === "Enter" && !event.shiftKey) {
			event.preventDefault();
			submit();
		} else if (event.key === "Backspace") {
			const chip = chipBeforeCaret(input);
			if (!chip) return;
			event.preventDefault();
			const next = chip.nextSibling;
			chip.remove();
			if (next) placeCaret(next, 0);
		}
	});
	input.addEventListener("input", () => {
		if (!composing) look();
	});
	input.addEventListener("compositionstart", () => {
		composing = true;
	});
	input.addEventListener("compositionend", () => {
		composing = false;
		look();
	});
	input.addEventListener("blur", close);
	return {
		input,
		list,
		isOpen: () => spot !== null,
		candidates: () => spot ? [...found] : [],
		activeIndex: () => spot ? active : -1,
		pieces: () => readPieces(input),
		text: () => serializePieces(readPieces(input)),
		clear: () => {
			input.replaceChildren();
			close();
		}
	};
}
// src/office/notice-view.ts
/** 공지 단추의 글. */
var NOTICE_BUTTON = "공지";
/** 공지 모드일 때 글 칸 옆에 보이는 표시. */
var NOTICE_MODE_LABEL = "공지로 보내기";
/** 공지 띠의 class. */
var NOTICE_BAND_CLASS = "notice-band";
/** 확인 상태 → 보이는 글. `answered` "✓ 답함", `waiting` · `queued` "기다림", `limit` "상한으로 답 못 함", `failed` "실패". */
function ackLabel(state) {
	switch (state) {
		case "answered": return "✓ 답함";
		case "limit": return "상한으로 답 못 함";
		case "failed": return "실패";
		default: return "기다림";
	}
}
/** 보낼 길: 공지 모드이고 멘션이 없으면 `"notice"`, 그 밖은 `"team"`(멘션 글 — 멘션이 없으면 서버가 안내를 돌려준다). */
function sendRoute(noticeMode, hasMention) {
	return noticeMode && !hasMention ? "notice" : "team";
}
var ACK_STATES = [
	"waiting",
	"queued",
	"answered",
	"limit",
	"failed"
];
/** 한 번 바뀌면 더 바뀌지 않는 확인 상태. */
var SETTLED = [
	"answered",
	"limit",
	"failed"
];
function record(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}
function ackState(value) {
	return typeof value === "string" && ACK_STATES.includes(value) ? value : null;
}
/** 공지 한 줄(목록 · 이벤트의 모양). 틀리면 `null`. */
function noticeFrom(value) {
	const row = record(value);
	if (!row || typeof row.notice !== "number" || typeof row.text !== "string" || typeof row.at !== "number") return null;
	const acks = [];
	for (const raw of Array.isArray(row.acks) ? row.acks : []) {
		const ack = record(raw);
		const state = ackState(ack?.state);
		if (ack && typeof ack.agent === "string" && state) acks.push({
			agent: ack.agent,
			state
		});
	}
	return {
		notice: row.notice,
		text: row.text,
		at: row.at,
		acks
	};
}
/**
* `office.notice` 내용 → 새 공지(담당 `agents`가 모두 `waiting`). 모양이 틀리면 `null`.
* 이미 같은 번호가 `items`에 있으면 `items`를 그대로, 없으면 끝에 더한 새 목록을 돌려준다.
*/
function applyNoticeEvent(items, payload, agents) {
	const item = noticeFrom(payload);
	if (!item || items.some((i) => i.notice === item.notice)) return [...items];
	item.acks = agents.map((agent) => ({
		agent,
		state: "waiting"
	}));
	return [...items, item];
}
/**
* `office.notice_ack` 내용 → 그 공지의 그 담당 확인을 바꾼 새 목록. 확인은 한 번만 바뀐다: 이미 `answered` ·
* `limit` · `failed`인 확인은 그대로다. 모르는 공지 · 담당 · 모양이 틀린 내용이면 `items` 그대로.
*/
function applyAckEvent(items, payload) {
	const row = record(payload);
	const state = ackState(row?.state);
	if (!row || !state || typeof row.notice !== "number" || typeof row.agent !== "string") return [...items];
	return items.map((item) => {
		if (item.notice !== row.notice) return item;
		const acks = item.acks.map((ack) => ack.agent === row.agent && !SETTLED.includes(ack.state) ? {
			agent: ack.agent,
			state
		} : ack);
		return {
			...item,
			acks
		};
	});
}
/** `GET` [`API.officeNotices`]의 본문(최근 것 먼저) → 보낸 순서(오래된 것 먼저)의 공지들. 모양이 틀린 줄은 버린다. */
function noticesFromList(body) {
	if (!Array.isArray(body)) return [];
	return body.map(noticeFrom).filter((item) => item !== null).reverse();
}
/**
* 공지 띠: `div.notice-band`(`data-notice`) 안에 공지 글(`.notice-text`)과 담당별 확인 줄(`.notice-acks` 안의
* `span.ack`, `data-agent` · `data-state`, 글 `"{이름} {ackLabel}"`). 이름은 `names`에서, 없으면 id.
*/
function noticeElement(item, names) {
	const band = document.createElement("div");
	band.className = NOTICE_BAND_CLASS;
	band.dataset.notice = String(item.notice);
	const tag = document.createElement("span");
	tag.className = "notice-tag";
	tag.textContent = NOTICE_BUTTON;
	const text = document.createElement("div");
	text.className = "notice-text";
	text.textContent = item.text;
	const acks = document.createElement("div");
	acks.className = "notice-acks";
	for (const ack of item.acks) {
		const chip = document.createElement("span");
		chip.className = "ack";
		chip.dataset.agent = ack.agent;
		chip.dataset.state = ack.state;
		chip.textContent = `${names[ack.agent] ?? ack.agent} ${ackLabel(ack.state)}`;
		acks.append(chip);
	}
	band.append(tag, text, acks);
	return band;
}
/** `host`에 공지 단추(글 [`NOTICE_BUTTON`], `aria-pressed`)와 모드 표시([`NOTICE_MODE_LABEL`])를 단다. 누르면 모드를 뒤집고 `onChange`. */
function mountNoticeToggle(host, onChange) {
	const button = document.createElement("button");
	button.type = "button";
	button.className = "notice-toggle";
	button.textContent = NOTICE_BUTTON;
	const marker = document.createElement("span");
	marker.className = "notice-mode";
	marker.textContent = NOTICE_MODE_LABEL;
	let on = false;
	const set = (next) => {
		on = next;
		button.setAttribute("aria-pressed", on ? "true" : "false");
		button.classList.toggle("on", on);
		marker.hidden = !on;
	};
	set(false);
	button.addEventListener("click", () => {
		set(!on);
		onChange(on);
	});
	host.append(button, marker);
	return {
		button,
		marker,
		isOn: () => on,
		set
	};
}
/** 공지를 보낸다: `POST` [`API.officeNotice`] `{"text", "sourceMessageId"}`(JSON, 같은 출처). 던지지 않는다. */
async function postNotice(fetcher, text, sourceMessageId) {
	try {
		const reply = await fetcher(API.officeNotice, {
			method: "POST",
			credentials: "same-origin",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				text,
				sourceMessageId
			})
		});
		const body = record(await reply.json().catch(() => null));
		if (!reply.ok || !body || typeof body.notice !== "number") return {
			ok: false,
			error: typeof body?.error === "string" ? body.error : `http_${reply.status}`
		};
		const acks = noticeFrom({
			notice: body.notice,
			text: "",
			at: 0,
			acks: body.acks
		})?.acks ?? [];
		return {
			ok: true,
			notice: body.notice,
			acks
		};
	} catch {
		return {
			ok: false,
			error: "network"
		};
	}
}
/** 지난 공지를 읽는다: `GET` [`API.officeNotices`]`?limit=<limit>` → [`noticesFromList`]. 실패하면 빈 목록(던지지 않는다). */
async function loadNotices(fetcher, limit = 20) {
	try {
		const reply = await fetcher(`${API.officeNotices}?limit=${limit}`, { credentials: "same-origin" });
		if (!reply.ok) return [];
		return noticesFromList(await reply.json());
	} catch {
		return [];
	}
}
// src/office/team-compose.ts
/** 공지 보내기 실패 안내. */
var NOTICE_FAILED = {
	too_fast: "조금 뒤에 다시 보내세요",
	text_too_long: "공지가 너무 깁니다"
};
/** 보낼 글 `@[이름]`을 채팅에 보일 `@이름`으로(가림 `\`을 푼다). */
function showMentions(text) {
	return text.replace(/@\[((?:\\.|[^\]\\])*)\]/g, (_, name) => `@${name.replace(/\\(.)/g, "$1")}`);
}
/** 시각 순서로 줄들 사이에 공지 띠를 끼운다(같은 시각이면 줄이 먼저). */
function interleave(rows, extra) {
	const out = [];
	let i = 0;
	const sorted = [...extra].sort((a, b) => a.at - b.at);
	for (const row of rows) {
		while (i < sorted.length && sorted[i].at < row.at) {
			out.push(sorted[i].el);
			i += 1;
		}
		out.push(row.el);
	}
	for (; i < sorted.length; i += 1) out.push(sorted[i].el);
	return out;
}
function mountTeamCompose(options) {
	const { form, textarea } = options;
	const doc = form.ownerDocument;
	const hint = doc.createElement("div");
	hint.className = "compose-hint";
	hint.setAttribute("role", "status");
	hint.setAttribute("aria-live", "polite");
	hint.hidden = true;
	const box = doc.createElement("div");
	box.className = "team-compose";
	const tools = doc.createElement("div");
	tools.className = "team-tools";
	form.prepend(hint);
	textarea.after(box);
	box.after(tools);
	let notices = [];
	let sending = false;
	const say = (text) => {
		hint.textContent = text ?? "";
		hint.hidden = !text;
	};
	const editor = mountMentionEditor(box, {
		members: options.members,
		onSubmit: () => submit()
	});
	editor.input.setAttribute("aria-label", "팀 방에 쓰기 (@로 담당 부르기)");
	editor.input.dataset.placeholder = "@로 담당을 부르세요 (Enter 보내기)";
	editor.input.addEventListener("keydown", (e) => e.stopPropagation());
	editor.input.addEventListener("keyup", (e) => e.stopPropagation());
	editor.input.addEventListener("input", () => say(null));
	const toggle = mountNoticeToggle(tools, (on) => {
		editor.input.classList.toggle("notice-on", on);
		editor.input.focus();
	});
	async function sendNotice(text) {
		sending = true;
		const id = globalThis.crypto?.randomUUID?.() ?? `n-${Date.now().toString(36)}`;
		const reply = await postNotice(options.fetcher, text, id);
		sending = false;
		if (!reply.ok) {
			say(`공지를 보내지 못했습니다 — ${NOTICE_FAILED[reply.error] ?? reply.error}`);
			return;
		}
		editor.clear();
		toggle.set(false);
		editor.input.classList.remove("notice-on");
	}
	function submit() {
		const text = editor.text();
		if (!text.trim() || !options.connected() || sending) return;
		say(null);
		if (sendRoute(toggle.isOn(), hasMention(editor.pieces())) === "notice") {
			sendNotice(text);
			return;
		}
		editor.clear();
		options.sendTeam(text);
	}
	return {
		setTeam(on) {
			box.hidden = !on;
			tools.hidden = !on;
			textarea.hidden = on;
			if (!on) say(null);
		},
		say,
		submit,
		async load() {
			const loaded = await loadNotices(options.fetcher);
			const known = new Set(notices.map((n) => n.notice));
			notices = [...loaded.filter((n) => !known.has(n.notice)), ...notices].sort((a, b) => a.at - b.at);
			options.changed();
		},
		onNotice(payload) {
			notices = applyNoticeEvent(notices, payload, options.agents);
			options.changed();
		},
		onAck(payload) {
			notices = applyAckEvent(notices, payload);
			options.changed();
		},
		bands: () => notices.map((item) => ({
			at: item.at,
			el: noticeElement(item, options.names())
		}))
	};
}
/** "누구와 대화?" 목록의 제목. */
var WHO_TO_TALK = "누구와 대화?";
/** 두 자리의 곧은 거리(칸). */
function distance(a, b) {
	return Math.hypot(a.x - b.x, a.y - b.y);
}
/** `player`에서 [`TALK_RADIUS`] 안의 담당 id, 가까운 순(같으면 id 글자 순). */
function nearbyAgents(player, agents) {
	return agents.map((a) => ({
		agent: a.agent,
		d: distance(player, a)
	})).filter((a) => a.d < 2).sort((a, b) => a.d - b.d || (a.agent < b.agent ? -1 : a.agent > b.agent ? 1 : 0)).map((a) => a.agent);
}
/** 목적격 조사: 마지막 글자가 받침 있는 한글이면 "을", 받침 없는 한글이면 "를", 한글이 아니면 "을(를)". */
function objectParticle(name) {
	const code = name.trim().slice(-1).charCodeAt(0);
	if (!(code >= 44032 && code <= 55203)) return "을(를)";
	return (code - 44032) % 28 === 0 ? "를" : "을";
}
/** 안내 글: `{이름}{을/를} 클릭하거나 /를 눌러 대화`. */
function talkHint(name) {
	return `${name}${objectParticle(name)} 클릭하거나 /를 눌러 대화`;
}
/** 가까운 담당(가까운 순) → `/`의 할 일: 없으면 none, 하나면 open, 둘 이상이면 choose(가까운 순 그대로). */
function slashAction(near) {
	if (near.length === 0) return { kind: "none" };
	if (near.length === 1) return {
		kind: "open",
		agent: near[0]
	};
	return {
		kind: "choose",
		agents: [...near]
	};
}
/** 대화 중인 담당에게서 멀어졌는가(거리가 [`TALK_RADIUS`] 이상이면 닫는다 — 기본 흐름 7). */
function tooFar(player, agent) {
	return distance(player, agent) >= 2;
}
/**
* 담당 누르기. `grid`는 담당이 선 칸이 막힌 표(`withBlockers`), `player`는 실수 자리, `agent`는 그 담당이 선 칸(정수).
* 거리는 `player`에서 그 칸 가운데(+0.5)까지다. 걸어갈 칸은 `besideCell`(내 칸에서 길이 가장 짧은 이웃 칸).
*/
function agentClick(grid, player, agent) {
	if (distance(player, {
		x: agent.x + .5,
		y: agent.y + .5
	}) < 2) return { kind: "open" };
	const to = besideCell(grid, cellOf(player), agent);
	return to === null ? { kind: "none" } : {
		kind: "walk",
		to
	};
}
// src/office/toolbar.ts
/** 도구 줄(순서대로): id와 단추 글. */
var TOOLS = [
	{
		id: "overview",
		label: "전체 보기"
	},
	{
		id: "rotate-left",
		label: "왼쪽 돌리기"
	},
	{
		id: "rotate-right",
		label: "오른쪽 돌리기"
	},
	{
		id: "perspective",
		label: "입체"
	},
	{
		id: "top",
		label: "위에서"
	},
	{
		id: "follow",
		label: "따라가기"
	},
	{
		id: "zoom-in",
		label: "+"
	},
	{
		id: "zoom-out",
		label: "−"
	}
];
/** 조작 안내 글. */
var CONTROLS_HINT = "방향키 이동 · / 대화 · 끌기 화면 이동";
/**
* `root` 안에 도구 줄을 단다: [`TOOLS`] 순서의 `<button type="button" data-tool="{id}">{label}</button>` 8개.
* 단추를 누르면 `onTool(id)`. "따라가기" 단추는 `aria-pressed`로 켜짐을 보인다(처음 `following`).
*/
function mountToolbar(root, onTool, following = true) {
	const doc = root.ownerDocument;
	const bar = doc.createElement("div");
	bar.className = "toolbar";
	bar.setAttribute("role", "toolbar");
	bar.setAttribute("aria-label", "카메라");
	let follow = null;
	for (const tool of TOOLS) {
		const button = doc.createElement("button");
		button.type = "button";
		button.dataset.tool = tool.id;
		button.textContent = tool.label;
		button.addEventListener("click", () => onTool(tool.id));
		if (tool.id === "follow") follow = button;
		bar.appendChild(button);
	}
	const setFollowing = (on) => {
		follow?.setAttribute("aria-pressed", on ? "true" : "false");
	};
	setFollowing(following);
	root.appendChild(bar);
	return {
		element: bar,
		setFollowing
	};
}
/** `root` 안에 조작 안내(`id="controls-hint"`, 글 [`CONTROLS_HINT`])를 달고 그 요소를 돌려준다. */
function mountControlsHint(root) {
	const hint = root.ownerDocument.createElement("div");
	hint.id = "controls-hint";
	hint.textContent = CONTROLS_HINT;
	root.appendChild(hint);
	return hint;
}
/**
* "누구와 대화?" 목록(3a)을 `root` 안에 그린다(있던 목록은 바꾼다): 제목 "누구와 대화?"와 담당마다 단추 하나
* (넘긴 순서 = 가까운 순, `data-agent="{id}"`, 글은 이름). 단추를 누르면 `onPick(id)`. 목록 요소를 돌려준다.
*/
function showChooser(root, agents, onPick) {
	const doc = root.ownerDocument;
	for (const old of root.querySelectorAll(".chooser")) old.remove();
	const list = doc.createElement("div");
	list.className = "chooser";
	list.setAttribute("role", "dialog");
	list.setAttribute("aria-label", WHO_TO_TALK);
	const title = doc.createElement("div");
	title.className = "chooser-title";
	title.textContent = WHO_TO_TALK;
	list.appendChild(title);
	for (const agent of agents) {
		const button = doc.createElement("button");
		button.type = "button";
		button.dataset.agent = agent.id;
		button.textContent = agent.name;
		button.addEventListener("click", () => onPick(agent.id));
		list.appendChild(button);
	}
	root.appendChild(list);
	return list;
}
/**
* 화면 아래 안내(말 걸기 안내 글)를 보이거나(`text`) 숨긴다(`null`). `el.hidden`과 `textContent`만 바꾼다.
*/
function setHint(el, text) {
	if (text === null) {
		el.hidden = true;
		return;
	}
	if (el.textContent !== text) el.textContent = text;
	el.hidden = false;
}
/** 발언자 샷으로 가는 전환 시간(밀리초, ease-in-out). */
var SPEAKER_TRANSITION_MS = 1200;
/** 탁자 샷으로 가는 전환 시간(밀리초, ease-in-out). */
var TABLE_TRANSITION_MS = 1200;
/** 한 발언자 샷의 최소 머무름(밀리초). */
var MIN_DWELL_MS = 2e3;
/** 마지막 말 뒤 발언자 샷을 유지하는 시간(밀리초). 그 안에 다른 담당이 말하면 탁자 샷을 거치지 않고 옮긴다. */
var HOLD_AFTER_TALK_MS = 4e3;
/** (25차 1번, 결정 18) 발언자 샷의 보는 중심: 탁자 샷의 중심에서 발언자 쪽으로 이 몫만 옮긴다. */
var SPEAKER_CENTER_SHARE = .5;
/** (25차 1번, 결정 18) 발언자 샷의 거리: 탁자 샷 거리의 이 배수. */
var SPEAKER_DISTANCE_SCALE = .8;
/**
* (25차 1번, 결정 18) 탁자 샷 궤도에서 발언자 샷 궤도를 낸다. 돌지 않는다: 방위각(theta) · 고도(polar)는
* 탁자 샷 그대로다. 보는 중심의 바닥 좌표(tx, tz)만 발언자 자리 쪽으로 [`SPEAKER_CENTER_SHARE`]만큼 옮기고
* (높이 ty는 그대로), 거리는 탁자 샷의 [`SPEAKER_DISTANCE_SCALE`]배다. 넘긴 값은 바꾸지 않는다.
*/
function speakerOrbit(table, spot) {
	return {
		...table,
		radius: table.radius * SPEAKER_DISTANCE_SCALE,
		tx: table.tx + (spot.x - table.tx) * SPEAKER_CENTER_SHARE,
		tz: table.tz + (spot.z - table.tz) * SPEAKER_CENTER_SHARE
	};
}
/**
* 발언자 카메라 상태 기계. 기본 켜짐.
* - 담당들이 미팅룸에 모여 있을 때(`gather(true)`) 논의 글이 오면(`talk`) 그 담당의 발언자 샷(고도는 탁자 샷 그대로
*   30°, 1.2초 — 궤도는 [`speakerOrbit`]).
* - 다른 담당이 말해도 지금 샷이 시작된 지 2초가 지나기 전에는 바꾸지 않는다(2초가 되는 때 바꾼다). 말 뒤 유지
*   안이면 탁자 샷을 거치지 않고 곧바로 그 담당 쪽으로 옮긴다.
* - 마지막 말 뒤 4초(그리고 그 샷의 최소 머무름 2초)가 지나면 탁자 샷(고도 30°, 1.2초).
* - 사용자가 카메라를 손으로 움직이면(`manual`) 그 논의 동안(`gather(false)`가 올 때까지) 꺼진다.
* - 단추(`setEnabled`)로 켜고 끈다. 꺼져 있으면 목표를 내지 않는다.
* - 목표가 없으면(논의 글 전 · 모이지 않음 · 꺼짐) [`goal`]은 `null`이고 장면은 평소 카메라를 쓴다.
*/
var SpeakerCamera = class {
	on = true;
	/** (32차 6번) 카메라가 내 캐릭터를 따라가는 중(기본 켜짐). 켜져 있으면 끼어들지 않는다. */
	follow = true;
	gathered = false;
	/** 사용자가 손으로 움직여 이 논의 동안 꺼졌다. */
	handOff = false;
	/** 지금 샷(없으면 `null`). */
	shot = null;
	/** 최소 머무름이 끝나면 넘어갈 다음 발언자. */
	next = null;
	lastTalkAt = 0;
	/** 자동 카메라 단추가 켜져 있는가. */
	enabled() {
		return this.on;
	}
	/** 단추: 켜고 끈다. */
	setEnabled(on, now) {
		this.on = on;
		if (!on) this.reset();
	}
	/**
	* (32차 6번) 카메라가 내 캐릭터를 따라가는 중인가. 기본 켜짐(사무실의 따라가기 기본값과 같다).
	*/
	following() {
		return this.follow;
	}
	/**
	* (32차 6번) 따라가기 켜짐 · 꺼짐을 알린다. 켜져 있으면 목표를 내지 않고 논의 글에도 움직이지 않는다(내 캐릭터가
	* 우선 — 지금 샷이 있으면 곧바로 손을 뗀다). 꺼지면 다음 논의 글부터 지금처럼 돈다. 단추(`enabled`)는 그대로다.
	*/
	setFollowing(on, now) {
		this.follow = on;
		if (on) this.reset();
	}
	/** 담당들이 미팅룸에 모여 있는가. `false`면 논의가 끝난 것이다(손 조작으로 꺼진 것도 풀린다). */
	gather(gathered, now) {
		if (gathered === this.gathered) return;
		this.gathered = gathered;
		if (!gathered) {
			this.handOff = false;
			this.reset();
		}
	}
	/** 논의 글이 왔다. */
	talk(agent, now) {
		if (!this.active()) return;
		this.advance(now);
		this.lastTalkAt = now;
		const shot = this.shot;
		if (!shot || shot.kind === "table") {
			this.shot = {
				kind: "speaker",
				agent,
				since: now
			};
			this.next = null;
		} else if (shot.agent === agent) this.next = null;
		else if (now >= shot.since + 2e3) {
			this.shot = {
				kind: "speaker",
				agent,
				since: now
			};
			this.next = null;
		} else this.next = agent;
	}
	/** 사용자가 카메라를 손으로 움직였다. */
	manual(now) {
		if (!this.gathered) return;
		this.handOff = true;
		this.reset();
	}
	/** `now`의 목표(없으면 `null`). */
	goal(now) {
		if (!this.active()) return null;
		this.advance(now);
		const shot = this.shot;
		if (!shot) return null;
		return shot.kind === "speaker" ? {
			shot: "speaker",
			agent: shot.agent,
			elevationDeg: 30,
			transitionMs: SPEAKER_TRANSITION_MS,
			since: shot.since
		} : {
			shot: "table",
			agent: null,
			elevationDeg: 30,
			transitionMs: TABLE_TRANSITION_MS,
			since: shot.since
		};
	}
	active() {
		return this.on && !this.follow && this.gathered && !this.handOff;
	}
	reset() {
		this.shot = null;
		this.next = null;
	}
	/** `now`까지 시간이 흐른 만큼 샷을 넘긴다: 기다리던 발언자(최소 머무름 뒤) → 탁자 샷(말 뒤 4초 · 최소 머무름 뒤). */
	advance(now) {
		const shot = this.shot;
		if (!shot || shot.kind !== "speaker") return;
		if (this.next !== null) {
			const at = shot.since + MIN_DWELL_MS;
			if (now < at) return;
			this.shot = {
				kind: "speaker",
				agent: this.next,
				since: at
			};
			this.next = null;
			this.advance(now);
			return;
		}
		const tableAt = Math.max(this.lastTalkAt + HOLD_AFTER_TALK_MS, shot.since + MIN_DWELL_MS);
		if (now >= tableAt) this.shot = {
			kind: "table",
			agent: null,
			since: tableAt
		};
	}
};
/** ease-in-out 보간(0 → 0, 1 → 1, 가운데 0.5, 양 끝이 느리다). 0~1 밖은 끝값으로 자른다. */
function easeInOut(t) {
	const x = Math.max(0, Math.min(1, t));
	return x < .5 ? 2 * x * x : 1 - (-2 * x + 2) ** 2 / 2;
}
/**
* `lines`가 `keep`줄을 넘으면 앞(오래된 쪽)에서 넘친 만큼 버린다(`lines`를 그 자리에서 고친다).
* 버린 응답 줄의 응답이 끝난 것(`complete` · `failed`)이면 `responses`에서도 지운다.
*/
function trimLines(lines, responses, keep = 200) {
	if (lines.length <= keep) return;
	for (const line of lines.splice(0, lines.length - keep)) {
		if (line.kind !== "resp") continue;
		const status = responses.get(line.requestId)?.status;
		if (status === "complete" || status === "failed") responses.delete(line.requestId);
	}
}
// \0vite/preload-helper.js
var scriptRel = /* @__PURE__ */ (function detectScriptRel() {
	const relList = typeof document !== "undefined" && document.createElement("link").relList;
	return relList && relList.supports && relList.supports("modulepreload") ? "modulepreload" : "preload";
})();
var assetsURL = function(dep) {
	return "/" + dep;
};
var seen = {};
var isCssPreloadUrl = function isCssPreloadUrl(url) {
	return url.pathname.endsWith(".css");
};
var preloadOnce = function preloadOnce(seen, href, preload) {
	if (href in seen) return seen[href];
	const promise = preload();
	if (!promise) {
		seen[href] = void 0;
		return;
	}
	const preloadPromise = promise.then(() => {
		seen[href] = void 0;
	}, (err) => {
		seen[href] = void 0;
		throw err;
	});
	seen[href] = preloadPromise;
	return preloadPromise;
};
var __vitePreload = function preload(baseModule, deps, importerUrl) {
	let promise = Promise.resolve();
	if (deps && deps.length > 0) {
		let preloadedHrefs;
		const cspNonceMeta = document.querySelector("meta[property=csp-nonce]");
		const cspNonce = cspNonceMeta?.nonce || cspNonceMeta?.getAttribute("nonce");
		function allSettled(promises) {
			return Promise.all(promises.map((p) => Promise.resolve(p).then((value) => ({
				status: "fulfilled",
				value
			}), (reason) => ({
				status: "rejected",
				reason
			}))));
		}
		function importMetaResolve(specifier) {
			if (import.meta.resolve) return new URL(import.meta.resolve(specifier));
			return new URL(
				specifier,
				/** #__KEEP__ */
				import.meta.url
			);
		}
		promise = allSettled(deps.map((depString) => {
			depString = assetsURL(depString, importerUrl);
			const dep = importMetaResolve(depString);
			const isCss = isCssPreloadUrl(dep);
			return preloadOnce(seen, dep.href, () => {
				if (preloadedHrefs === void 0) {
					preloadedHrefs = {
						all: /* @__PURE__ */ new Set(),
						styles: /* @__PURE__ */ new Set()
					};
					const links = document.getElementsByTagName("link");
					for (let i = links.length - 1; i >= 0; i--) {
						const link = links[i];
						preloadedHrefs.all.add(link.href);
						if (link.rel === "stylesheet") preloadedHrefs.styles.add(link.href);
					}
				}
				if ((isCss ? preloadedHrefs.styles : preloadedHrefs.all).has(dep.href)) return;
				const link = document.createElement("link");
				link.rel = isCss ? "stylesheet" : scriptRel;
				if (!isCss) link.as = "script";
				link.crossOrigin = "";
				link.href = dep.href;
				if (cspNonce) link.setAttribute("nonce", cspNonce);
				document.head.appendChild(link);
				if (isCss) return new Promise((res, rej) => {
					const handleSettle = (e) => {
						link.removeEventListener("load", handleSettle);
						link.removeEventListener("error", handleSettle);
						if (e.type === "error") rej(/* @__PURE__ */ new Error(`Unable to preload CSS for ${dep}`));
						else res(e);
					};
					link.addEventListener("load", handleSettle);
					link.addEventListener("error", handleSettle);
				});
			});
		}).filter((p) => p !== void 0));
	}
	function handlePreloadError(err) {
		const e = new Event("vite:preloadError", { cancelable: true });
		e.payload = err;
		window.dispatchEvent(e);
		if (!e.defaultPrevented) throw err;
	}
	return promise.then((res) => {
		for (const item of res || []) {
			if (item.status !== "rejected") continue;
			handlePreloadError(item.reason);
		}
		return baseModule().catch(handlePreloadError);
	});
};
// src/office/main.ts
var SESSION_PATH = API.session;
var SNAPSHOT_PATH = API.office;
var CHAT_PATH = API.officeChat;
var WS_PATH = API.socket;
var MAP_PATH = "./office-map.json";
var RETRY_MIN_MS = 1e3;
var RETRY_MAX_MS = 1e4;
var FOLLOW_PX = 80;
var REPLAY_WINDOW_MS = 3e3;
var CAPTION_TICK_MS = 500;
var AGENT_IDS = [
	"news",
	"strategy",
	"risk",
	"review",
	"trader",
	"leader"
];
var PLACE_LABEL = {
	bed: "라운지 쉼 자리",
	desk: "사무실 자기 자리",
	meeting: "미팅룸 탁자"
};
var WORK_LABEL = {
	idle: "",
	queued: "대기",
	thinking: "생각 중",
	failed: "실패"
};
var ROOM_TEAM = "team";
var $ = (id) => document.getElementById(id);
var state = {
	map: null,
	names: {},
	agents: new Map(AGENT_IDS.map((id) => [id, {
		place: "bed",
		work: "idle"
	}])),
	responses: /* @__PURE__ */ new Map(),
	lines: /* @__PURE__ */ new Map(),
	conversation: ROOM_TEAM,
	lastSeq: null,
	connected: false,
	view: null,
	looks: { ...DEFAULT_LOOKS },
	looksLoaded: false,
	me: null,
	meLoaded: false
};
/** (22차) 팀의 대화: 말풍선 판 · 최근 말 · 팀 탭의 논의 줄. */
var chatter = new Chatter();
/** (23차 7번 (나)) 회의 발언자 카메라(기본 켜짐). */
var speaker = new SpeakerCamera();
/** (32차 6번) 카메라 따라가기(기본 켜짐). 켜져 있으면 발언자 카메라가 끼어들지 않는다. */
var follow = new FollowState();
var toolbar = null;
/** (33차) 팀 방 글 칸(멘션 칩 · 공지)과 부르기. */
var compose = null;
var summons = null;
/** (34차) 머리 위 ✋ · ✓ · 승인 알림음 · 범례. */
var marks = null;
/** 이 시각까지 받은 `office.talk`은 다시 이을 때 다시 받은 것일 수 있다. */
var replayUntil = 0;
function renderLight() {
	const list = $("light-list");
	list.replaceChildren();
	for (const id of AGENT_IDS) {
		const a = state.agents.get(id);
		const li = document.createElement("li");
		const button = document.createElement("button");
		button.type = "button";
		const name = document.createElement("strong");
		const work = WORK_LABEL[a.work] ? ` · ${WORK_LABEL[a.work]}` : "";
		name.textContent = `${state.names[id] ?? id}${work}`;
		const where = document.createElement("span");
		where.className = "where";
		where.textContent = PLACE_LABEL[a.place] ?? a.place;
		button.append(name, where);
		const said = chatter.latest(id);
		if (said) {
			const line = document.createElement("span");
			line.className = "said";
			line.textContent = said;
			button.append(line);
		}
		button.addEventListener("click", () => selectConversation(id));
		li.append(button, ...marks?.badges(id, Date.now()) ?? [], summonButton(id, (agent) => void summons?.summon(agent)));
		list.append(li);
	}
}
function showLight(reason) {
	if (state.view) {
		try {
			state.view.dispose();
		} catch {}
		state.view = null;
	}
	$("stage").hidden = true;
	$("light").hidden = false;
	const toggle = $("view-toggle");
	toggle.hidden = !reason || reason === "lost" ? true : false;
	renderLight();
}
async function show3d() {
	const { webglAvailable, createScene } = await __vitePreload(async () => {
		const { webglAvailable, createScene } = await import("./scene3d-cxybrbzk.js");
		return {
			webglAvailable,
			createScene
		};
	}, []);
	if (!webglAvailable()) throw new Error("webgl");
	$("stage").hidden = false;
	$("light").hidden = true;
	const scene = createScene({
		host: $("canvas-host"),
		labels: $("labels"),
		map: state.map,
		names: state.names,
		looks: state.looks,
		onPick: (id) => selectConversation(id),
		onLost: () => showLight("lost"),
		follow,
		player: {
			stage: $("stage"),
			me: state.me,
			hooks: {
				openChat: (id) => selectConversation(id),
				closeChat: (id) => {
					if (state.conversation === id) selectConversation(ROOM_TEAM);
				},
				conversation: () => state.conversation,
				fetcher: (input, init) => fetch(input, init),
				onSpot: (spot) => {
					if (state.me) state.me = {
						...state.me,
						...spot
					};
				}
			}
		},
		speech: chatter.board,
		speaker,
		marks: marks?.layer($("stage"))
	});
	state.view = scene;
	for (const [id, view] of state.agents) scene.setAgent(id, view, { snap: true });
	summons?.redraw();
}
function applyAgent(view, snap) {
	if (!view || !state.agents.has(view.agent)) return;
	const next = {
		place: view.place,
		work: view.work
	};
	const before = state.agents.get(view.agent)?.work ?? "idle";
	state.agents.set(view.agent, next);
	if (!snap) marks?.work(view.agent, before, next.work, Date.now());
	chatter.work(view.agent, next.work);
	if (state.view) state.view.setAgent(view.agent, next, { snap });
	if (!$("light").hidden) renderLight();
	renderTabs();
}
function convKeyOf(resp) {
	return resp.room === ROOM_TEAM ? ROOM_TEAM : resp.agent;
}
function linesOf(key) {
	if (!state.lines.has(key)) state.lines.set(key, []);
	return state.lines.get(key);
}
/** 응답 객체는 requestId별로 updatedAt이 크거나 같은 것만 받아 덮어쓴다(늦게 온 옛 상태는 버린다). */
function applyChat(resp) {
	if (!resp || typeof resp.requestId !== "string") return;
	const old = state.responses.get(resp.requestId);
	if (old && Number(old.updatedAt) > Number(resp.updatedAt)) return;
	state.responses.set(resp.requestId, resp);
	chatter.chat(resp);
	if (!old) {
		chatter.stamp(resp.requestId);
		const lines = linesOf(convKeyOf(resp));
		const at = lines.findIndex((l) => l.kind === "me" && l.id === resp.sourceMessageId);
		const entry = {
			kind: "resp",
			requestId: resp.requestId
		};
		if (at >= 0) {
			let end = at + 1;
			for (;;) {
				const next = lines[end];
				if (end >= lines.length || next.kind !== "resp" || state.responses.get(next.requestId)?.sourceMessageId !== resp.sourceMessageId) break;
				end++;
			}
			lines.splice(end, 0, entry);
		} else lines.push(entry);
		trimLines(lines, state.responses);
	}
	if (convKeyOf(resp) === state.conversation) renderLog(false);
	renderCaption();
}
function newId() {
	if (window.crypto?.randomUUID) return window.crypto.randomUUID();
	return `m-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
function nameOf(id) {
	return state.names[id] ?? id;
}
function renderTabs() {
	const tabs = $("chat-tabs");
	tabs.replaceChildren();
	for (const key of [ROOM_TEAM, ...AGENT_IDS]) {
		const b = document.createElement("button");
		b.type = "button";
		b.setAttribute("role", "tab");
		b.setAttribute("aria-selected", String(key === state.conversation));
		const glyph = key === ROOM_TEAM ? "" : {
			queued: " ⏳",
			thinking: " 💭",
			failed: " ❗"
		}[state.agents.get(key)?.work ?? ""] ?? "";
		b.textContent = (key === ROOM_TEAM ? "팀 방" : nameOf(key)) + glyph;
		b.addEventListener("click", () => selectConversation(key));
		tabs.append(b);
		if (key !== ROOM_TEAM) tabs.append(summonButton(key, (agent) => void summons?.summon(agent)));
	}
	$("chat-to-wrap").hidden = true;
	compose?.setTeam(state.conversation === ROOM_TEAM);
	renderCaption();
}
/** (23차 6번 · 7번 (가)) 화면 아래 자막과 팀 탭 위 `○○ 발언 중…`. 없으면 숨긴다. */
function renderCaption() {
	const line = chatter.caption(state.names);
	const cap = $("caption");
	if (cap.textContent !== (line ?? "")) {
		cap.textContent = line ?? "";
		cap.title = line ?? "";
	}
	cap.hidden = line === null;
	const speaking = state.conversation === ROOM_TEAM ? chatter.speaking(state.names) : null;
	const bar = $("speaking");
	if (bar.textContent !== (speaking ?? "")) bar.textContent = speaking ?? "";
	bar.hidden = speaking === null;
}
function selectConversation(key) {
	summons?.conversation(state.conversation, key);
	state.conversation = key;
	renderTabs();
	renderLog(true);
}
function lineElement(line) {
	if (line.kind === "me") {
		const div = chatLineElement(chatLine({
			kind: "me",
			key: line.id,
			text: showMentions(line.text)
		}, state.names), line.room === ROOM_TEAM && line.agent ? `→ ${nameOf(line.agent)}` : void 0);
		if (line.failed) {
			div.classList.add("failed");
			const retry = document.createElement("button");
			retry.type = "button";
			retry.className = "retry";
			retry.textContent = "다시 보내기";
			retry.addEventListener("click", () => {
				send(line.room, line.agent, line.text, line);
			});
			div.append(document.createTextNode("\n보내지 못함"), retry);
		}
		return div;
	}
	const resp = state.responses.get(line.requestId);
	if (!resp) return null;
	const view = chatLine({
		kind: "reply",
		key: resp.requestId,
		agent: resp.agent,
		status: resp.status,
		content: resp.content
	}, state.names);
	if (resp.status === "failed") return chatLineElement({
		...view,
		text: `실패 (${resp.error ?? "까닭 모름"})`
	}, void 0, "failed");
	return chatLineElement(view);
}
/** (22차 2번) 팀 탭: 내 글 · 담당 답 · 논의 줄을 시각 순서로. */
function teamElements(lines) {
	const byKey = /* @__PURE__ */ new Map();
	const entries = [];
	for (const line of lines) {
		const key = line.kind === "me" ? line.id : line.requestId;
		byKey.set(key, line);
		const resp = line.kind === "resp" ? state.responses.get(line.requestId) : void 0;
		entries.push(line.kind === "me" ? {
			kind: "me",
			key,
			at: chatter.stamp(key),
			agent: line.agent,
			text: line.text
		} : {
			kind: "reply",
			key,
			at: chatter.stamp(key),
			agent: resp?.agent ?? "",
			text: resp?.content ?? ""
		});
	}
	const talks = /* @__PURE__ */ new Map();
	for (const talk of chatter.talkEntries()) {
		talks.set(talk.key, talk);
		entries.push(talk);
	}
	return interleave(teamFeed(entries, state.names).map((row) => {
		const talk = talks.get(row.key);
		if (row.kind === "talk" && talk?.kind === "talk") {
			const view = chatLine({
				kind: "talk",
				key: talk.key,
				agent: talk.agent,
				text: talk.text,
				topic: talk.topic
			}, state.names);
			return {
				at: row.at,
				el: chatLineElement(view, void 0, "talk")
			};
		}
		const line = byKey.get(row.key);
		return {
			at: row.at,
			el: line ? lineElement(line) : null
		};
	}), compose?.bands() ?? []);
}
function renderLog(forceBottom) {
	const log = $("chat-log");
	const follow = forceBottom || log.scrollHeight - log.scrollTop - log.clientHeight <= FOLLOW_PX;
	log.replaceChildren();
	const lines = linesOf(state.conversation);
	const elements = state.conversation === ROOM_TEAM ? teamElements(lines) : lines.map(lineElement);
	for (const el of elements) if (el) log.append(el);
	if (follow) log.scrollTop = log.scrollHeight;
}
async function send(room, agent, text, existing) {
	const line = existing ?? {
		kind: "me",
		id: newId(),
		room,
		agent,
		text
	};
	chatter.stamp(line.id);
	line.failed = false;
	if (!existing) {
		const lines = linesOf(room === ROOM_TEAM ? ROOM_TEAM : agent);
		lines.push(line);
		trimLines(lines, state.responses);
	}
	renderLog(true);
	try {
		const reply = await fetch(CHAT_PATH, {
			method: "POST",
			credentials: "same-origin",
			cache: "no-store",
			headers: { "content-type": "application/json" },
			body: JSON.stringify(agent ? {
				room,
				agent,
				text,
				sourceMessageId: line.id
			} : {
				room,
				text,
				sourceMessageId: line.id
			})
		});
		if (!reply.ok) throw new Error(String(reply.status));
		const body = await reply.json();
		if (Array.isArray(body?.responses)) {
			for (const resp of body.responses) applyChat(resp);
			compose?.say(hintText(body.hint));
		} else applyChat(body);
	} catch {
		line.failed = true;
	}
	renderLog(true);
}
function wireChat() {
	const select = $("chat-to");
	for (const id of AGENT_IDS) {
		const option = document.createElement("option");
		option.value = id;
		option.textContent = nameOf(id);
		select.append(option);
	}
	select.value = "leader";
	const input = $("chat-input");
	const form = $("chat-form");
	compose = mountTeamCompose({
		form: $("chat-form"),
		textarea: input,
		members: () => AGENT_IDS.map((agent) => ({
			agent,
			name: nameOf(agent)
		})),
		names: () => state.names,
		agents: AGENT_IDS,
		connected: () => state.connected,
		sendTeam: (text) => void send(ROOM_TEAM, "", text),
		changed: () => {
			if (state.conversation === ROOM_TEAM) renderLog(false);
		},
		fetcher: (url, init) => fetch(url, {
			...init,
			cache: "no-store"
		})
	});
	const submit = () => {
		if (state.conversation === ROOM_TEAM) {
			compose?.submit();
			return;
		}
		const text = input.value;
		if (!text.trim() || !state.connected) return;
		const room = state.conversation === ROOM_TEAM ? ROOM_TEAM : "direct";
		const agent = room === ROOM_TEAM ? select.value : state.conversation;
		input.value = "";
		send(room, agent, text);
	};
	form.addEventListener("submit", (e) => {
		e.preventDefault();
		submit();
	});
	input.addEventListener("keydown", (e) => {
		e.stopPropagation();
		if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
			e.preventDefault();
			submit();
		}
	});
	input.addEventListener("keyup", (e) => e.stopPropagation());
}
var retryMs = RETRY_MIN_MS;
function setConn(text, kind) {
	const el = $("conn");
	el.textContent = text;
	el.dataset.state = kind;
	state.connected = kind === "ok";
	$("chat-form").setAttribute("aria-disabled", String(!state.connected));
	$("chat-send").disabled = !state.connected;
}
function onFrame(text) {
	let frame;
	try {
		frame = JSON.parse(text);
	} catch {
		return;
	}
	if (!frame || typeof frame !== "object") return;
	if (typeof frame.seq === "number") {
		if (state.lastSeq !== null && frame.seq <= state.lastSeq) return;
		state.lastSeq = frame.seq;
		if (frame.kind === "office.agent") applyAgent(frame.payload, false);
		else if (frame.kind === "office.chat") applyChat(frame.payload);
		else if (frame.kind === "office.talk") onTalk(frame.payload, frame.seq, !replayedOld(frame.payload));
		else if (frame.kind === "office.approval") {
			applyApproval($("approvals"), frame.payload, (input, init) => fetch(input, init), () => {
				refreshApprovals(false);
			});
			marks?.onApproval(frame.payload);
		} else if (frame.kind === "rule_proposal") refreshApprovals(true);
		else if (frame.kind === "office.notice") compose?.onNotice(frame.payload);
		else if (frame.kind === "office.notice_ack") compose?.onAck(frame.payload);
		else if (frame.kind === "office.summon") summons?.onEvent(frame.payload);
	}
}
/** 다시 이은 직후에 다시 받은 옛 논의 글인가(`at`이 말풍선 수명보다 오래됨). */
function replayedOld(payload) {
	const now = Date.now();
	if (now >= replayUntil) return false;
	const talk = readTalk(payload);
	return talk !== null && now - talk.at >= 12e3;
}
/** (22차) 팀 대화 한 줄: 팀 탭에 넣고, 실시간이면 말풍선 · 최근 말 · 자막 · 발언자 카메라에도. */
function onTalk(payload, seq, live) {
	if (!chatter.talk(payload, seq, live)) return;
	const talk = live ? readTalk(payload) : null;
	if (talk) speaker.talk(talk.agent, Date.now());
	renderCaption();
	if (state.conversation === ROOM_TEAM) renderLog(false);
	if (!$("light").hidden) renderLight();
}
/** 서버의 담당 → 외형을 받아 둔다(목록 밖 값은 버린다). */
function takeLooks(body) {
	const agents = body?.agents ?? {};
	for (const id of AGENT_IDS) {
		const look = agents[id];
		if (typeof look === "string") state.looks[id] = look;
	}
	return { ...state.looks };
}
function applyLooks() {
	if (state.view) for (const id of AGENT_IDS) state.view.setLook(id, state.looks[id]);
}
async function loadLooks() {
	try {
		const reply = await fetch(OFFICE_LOOK_PATHS.list, {
			credentials: "same-origin",
			cache: "no-store"
		});
		if (!reply.ok) return;
		takeLooks(await reply.json());
		state.looksLoaded = true;
		applyLooks();
	} catch {}
}
async function chooseLook(agent, look) {
	const reply = await fetch(OFFICE_LOOK_PATHS.choose, {
		method: "POST",
		credentials: "same-origin",
		cache: "no-store",
		headers: { "content-type": "application/json" },
		body: JSON.stringify({
			agent,
			look
		})
	});
	if (!reply.ok) throw new Error(String(reply.status));
	return takeLooks(await reply.json());
}
async function loadSnapshot() {
	try {
		const reply = await fetch(SNAPSHOT_PATH, {
			credentials: "same-origin",
			cache: "no-store"
		});
		if (!reply.ok) return;
		const snap = await reply.json();
		for (const view of snap.agents ?? []) {
			applyAgent(view, true);
			summons?.fromSnapshot(view.agent, view.phase, view.reason);
		}
		for (const resp of snap.pending ?? []) applyChat(resp);
		for (const talk of snap.talk ?? []) if (typeof talk.seq === "number") onTalk(talk, talk.seq, false);
		if (typeof snap.last_seq === "number") state.lastSeq = snap.last_seq;
		marks?.sync(snap.pending_approvals, false);
	} catch {}
}
/** (34차 1번 · 5번) 기다리는 승인만 다시 읽어 맞춘다(다시 이을 때 · 개선안 요청이 생겼을 때). */
async function refreshApprovals(ringNew) {
	try {
		const reply = await fetch(SNAPSHOT_PATH, {
			credentials: "same-origin",
			cache: "no-store"
		});
		if (!reply.ok) return;
		const snap = await reply.json();
		marks?.sync(snap.pending_approvals, ringNew);
	} catch {}
}
function retryLater() {
	const wait = retryMs;
	retryMs = Math.min(retryMs * 2, RETRY_MAX_MS);
	setTimeout(() => {
		start();
	}, wait);
}
function openSocket() {
	let url = `${location.protocol.replace("http", "ws")}//${location.host}${WS_PATH}`;
	const resuming = state.lastSeq !== null;
	if (resuming) url += `?last_seq=${encodeURIComponent(String(state.lastSeq))}`;
	let socket;
	try {
		socket = new WebSocket(url);
	} catch {
		retryLater();
		return;
	}
	let closed = false;
	const onClose = () => {
		if (closed) return;
		closed = true;
		setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
		retryLater();
	};
	socket.onopen = () => {
		retryMs = RETRY_MIN_MS;
		if (resuming) {
			replayUntil = Date.now() + REPLAY_WINDOW_MS;
			refreshApprovals(false);
		}
		setConn("연결됨", "ok");
	};
	socket.onmessage = (event) => {
		if (typeof event.data === "string") onFrame(event.data);
	};
	socket.onclose = onClose;
	socket.onerror = () => {
		try {
			socket.close();
		} catch {}
		onClose();
	};
}
async function start() {
	try {
		if (!(await fetch(SESSION_PATH, {
			method: "POST",
			credentials: "same-origin",
			cache: "no-store"
		})).ok) throw new Error("session");
		if (!state.looksLoaded) await loadLooks();
		if (!state.meLoaded) {
			state.me = await loadMe((input, init) => fetch(input, {
				...init,
				cache: "no-store"
			}));
			state.meLoaded = state.me !== null;
		}
		if (state.lastSeq === null) {
			await loadSnapshot();
			await compose?.load();
		}
		openSocket();
	} catch {
		setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
		retryLater();
	}
}
/** (23차 7번 (나)) 단추 "자동 카메라 켜짐/꺼짐"(기본 켜짐). */
function wireAutoCamera() {
	const button = $("auto-camera");
	const paint = () => {
		const on = speaker.enabled();
		button.setAttribute("aria-pressed", String(on));
		button.textContent = on ? "자동 카메라 켜짐" : "자동 카메라 꺼짐";
	};
	button.addEventListener("click", () => {
		speaker.setEnabled(!speaker.enabled(), Date.now());
		paint();
	});
	paint();
	button.hidden = false;
}
/** (32차 6번) 카메라 도구 줄 8개 · 오른쪽 아래 조작 안내. 따라가기 켜짐을 단추와 발언자 카메라에 알린다. */
function wireCameraTools() {
	const stage = $("stage");
	toolbar = mountToolbar(stage, (id) => state.view?.tool(id), follow.on());
	mountControlsHint(stage);
	follow.onChange((on) => {
		speaker.setFollowing(on, Date.now());
		toolbar?.setFollowing(on);
	});
}
async function main() {
	mountMenu(document.getElementById("menu"), "office");
	try {
		state.map = await (await fetch(MAP_PATH, { cache: "no-store" })).json();
	} catch {
		state.map = null;
	}
	for (const id of AGENT_IDS) state.names[id] = state.map?.agents?.[id]?.name ?? id;
	if (state.map) summons = createSummonScene({
		map: state.map,
		view: () => state.view,
		fetcher: (url, init) => fetch(url, {
			...init,
			cache: "no-store"
		}),
		nameOf,
		openChat: (id) => selectConversation(id),
		say: (text) => compose?.say(text)
	});
	const legend = document.createElement("div");
	legend.className = "legend-wrap";
	$("look-toggle").after(legend);
	marks = createOfficeMarks({
		approvals: $("approvals"),
		fetcher: (input, init) => fetch(input, init),
		names: state.names,
		legendRoot: legend,
		gestures: document
	});
	marks.onChange(() => {
		if (!$("light").hidden) renderLight();
	});
	wireChat();
	renderTabs();
	renderLog(true);
	setConn("잇는 중", "wait");
	const picker = createLookPicker({
		panel: $("looks"),
		agents: AGENT_IDS,
		nameOf,
		current: () => state.looks,
		choose: chooseLook,
		applied: () => applyLooks()
	});
	$("look-toggle").addEventListener("click", () => {
		if (!$("looks").hidden) {
			picker.close();
			return;
		}
		picker.open(state.conversation === ROOM_TEAM ? void 0 : state.conversation);
	});
	await start();
	const narrow = window.matchMedia("(max-width: 760px)").matches;
	const toggle = $("view-toggle");
	if (!state.map || narrow) {
		showLight(state.map ? "narrow" : "lost");
		toggle.textContent = "3D 사무실";
	} else try {
		await show3d();
		toggle.hidden = false;
		toggle.textContent = "가벼운 화면";
	} catch {
		showLight("lost");
	}
	wireCameraTools();
	wireAutoCamera();
	setInterval(renderCaption, CAPTION_TICK_MS);
	let lightSaid = "";
	setInterval(() => {
		if ($("light").hidden) return;
		const said = AGENT_IDS.map((id) => chatter.latest(id) ?? "").join("");
		if (said !== lightSaid) {
			lightSaid = said;
			renderLight();
		}
	}, 1e3);
	const onToggle = async () => {
		if (state.view) {
			showLight("chosen");
			toggle.hidden = false;
			toggle.textContent = "3D 사무실";
		} else try {
			await show3d();
			toggle.textContent = "가벼운 화면";
		} catch {
			showLight("lost");
		}
	};
	toggle.addEventListener("click", () => {
		onToggle();
	});
}
main();
export { bubbleWidth as A, followStep as C, bubbleCandidates as D, LANE_AREA_RATIO as E, overlap as M, bubbleText as O, FOLLOW_LERP as S, LOOKS as T, buildGrid as _, agentClick as a, SpotSender as b, slashAction as c, cellOf as d, entranceCell as f, ACTOR_RADIUS as g, withBlockers as h, showChooser as i, insideScreen as j, bubbleWhere as k, talkHint as l, startCell as m, speakerOrbit as n, distance as o, pathTo as p, setHint as r, nearbyAgents as s, easeInOut as t, tooFar as u, clearLine as v, DEFAULT_LOOKS as w, postSpot as x, findPath as y };
