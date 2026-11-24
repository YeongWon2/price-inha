import { a as approvalPath, n as API, o as decidePath, r as OFFICE_LOOK_PATHS, t as mountMenu } from "./menu-ijsrwa1d.js";
var PENDING = "pending";
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
function verbButton(root, number, verb, fetcher) {
	const button = document.createElement("button");
	button.type = "button";
	button.dataset.verb = verb;
	button.textContent = verb === "approve" ? "승인" : "거절";
	button.addEventListener("click", () => {
		decideApproval(root, number, verb, fetcher);
	});
	return button;
}
function newCard(root, number, fetcher) {
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
	row.append(verbButton(root, number, "approve", fetcher), verbButton(root, number, "reject", fetcher));
	const result = textPart("p", "result", "muted");
	result.setAttribute("role", "status");
	card.append(title, textPart("p", "summary"), code, row, result);
	root.append(card);
	return card;
}
/**
* 이벤트 한 건을 카드에 반영한다. `pending`이면 그 번호의 카드를 그리거나 고치고(같은 번호는 한 장),
* 아니면 그 카드를 닫는다(지운다). 모양이 맞지 않는 몸통은 무시한다.
*/
function applyApproval(root, payload, fetcher) {
	const approval = parse(payload);
	if (!approval) return;
	const existing = findCard(root, approval.number);
	if (approval.status !== PENDING) {
		existing?.remove();
		return;
	}
	const card = existing ?? newCard(root, approval.number, fetcher);
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
*/
async function decideApproval(root, number, verb, fetcher) {
	const card = findCard(root, number);
	if (card?.dataset.deciding === "1") return "failed";
	if (card) card.dataset.deciding = "1";
	lockVerbs(card, true);
	const outcome = await send$1(card, number, verb, fetcher);
	if (card) delete card.dataset.deciding;
	if (outcome !== "sent") lockVerbs(card, false);
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
/** 글 말풍선이 마지막으로 바뀐 뒤 보이는 시간(밀리초). */
var SPEECH_LIFE_MS = 12e3;
/** 글 없이 "말하는 중"만 알 때의 표시 글. */
var TYPING_TEXT = "···";
/** [`TYPING_TEXT`] 말풍선이 보이는 시간(밀리초). */
var TYPING_LIFE_MS = 4e3;
/** 좌우 후보가 비켜나는 거리(말풍선 폭의 배수). */
var BUBBLE_SIDE_SHIFT = .55;
/** 말풍선 넓이의 합이 화면 넓이의 이 비율을 넘으면 모두 말풍선 줄로 보낸다. */
var LANE_AREA_RATIO = .24;
/** 상태 표시: 생각 중 💭 · 쓰는 중 💬 · 기다림 ⏳ · 실패 ❗. 글 앞에 붙인다. */
var STATE_ICONS = {
	thinking: "💭",
	writing: "💬",
	waiting: "⏳",
	failed: "❗"
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
		return this.on && this.gathered && !this.handOff;
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
	looksLoaded: false
};
/** (22차) 팀의 대화: 말풍선 판 · 최근 말 · 팀 탭의 논의 줄. */
var chatter = new Chatter();
/** (23차 7번 (나)) 회의 발언자 카메라(기본 켜짐). */
var speaker = new SpeakerCamera();
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
		li.append(button);
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
		const { webglAvailable, createScene } = await import("./scene3d-oc5vbw0i.js");
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
		onFollow: (following) => {
			$("recenter").hidden = following;
		},
		speech: chatter.board,
		speaker
	});
	state.view = scene;
	$("recenter").hidden = true;
	for (const [id, view] of state.agents) scene.setAgent(id, view, { snap: true });
}
function applyAgent(view, snap) {
	if (!view || !state.agents.has(view.agent)) return;
	const next = {
		place: view.place,
		work: view.work
	};
	state.agents.set(view.agent, next);
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
	}
	$("chat-to-wrap").hidden = state.conversation !== ROOM_TEAM;
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
	state.conversation = key;
	renderTabs();
	renderLog(true);
}
function lineElement(line) {
	if (line.kind === "me") {
		const div = chatLineElement(chatLine({
			kind: "me",
			key: line.id,
			text: line.text
		}, state.names), line.room === ROOM_TEAM ? `→ ${nameOf(line.agent)}` : void 0);
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
	return teamFeed(entries, state.names).map((row) => {
		const talk = talks.get(row.key);
		if (row.kind === "talk" && talk?.kind === "talk") return chatLineElement(chatLine({
			kind: "talk",
			key: talk.key,
			agent: talk.agent,
			text: talk.text,
			topic: talk.topic
		}, state.names), void 0, "talk");
		const line = byKey.get(row.key);
		return line ? lineElement(line) : null;
	});
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
			body: JSON.stringify({
				room,
				agent,
				text,
				sourceMessageId: line.id
			})
		});
		if (!reply.ok) throw new Error(String(reply.status));
		applyChat(await reply.json());
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
	const submit = () => {
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
		else if (frame.kind === "office.approval") applyApproval($("approvals"), frame.payload, (input, init) => fetch(input, init));
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
		for (const view of snap.agents ?? []) applyAgent(view, true);
		for (const resp of snap.pending ?? []) applyChat(resp);
		for (const talk of snap.talk ?? []) if (typeof talk.seq === "number") onTalk(talk, talk.seq, false);
		if (typeof snap.last_seq === "number") state.lastSeq = snap.last_seq;
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
		if (resuming) replayUntil = Date.now() + REPLAY_WINDOW_MS;
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
		if (state.lastSeq === null) await loadSnapshot();
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
async function main() {
	mountMenu(document.getElementById("menu"), "office");
	try {
		state.map = await (await fetch(MAP_PATH, { cache: "no-store" })).json();
	} catch {
		state.map = null;
	}
	for (const id of AGENT_IDS) state.names[id] = state.map?.agents?.[id]?.name ?? id;
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
	$("recenter").addEventListener("click", () => {
		state.view?.recenter();
	});
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
	toggle.addEventListener("click", async () => {
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
	});
}
main();
export { LANE_AREA_RATIO as a, bubbleWhere as c, overlap as d, LOOKS as i, bubbleWidth as l, speakerOrbit as n, bubbleCandidates as o, DEFAULT_LOOKS as r, bubbleText as s, easeInOut as t, insideScreen as u };
