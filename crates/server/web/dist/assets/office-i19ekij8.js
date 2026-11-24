import { a as decidePath, i as approvalPath, n as OFFICE_LOOK_PATHS, t as API } from "./paths-sc36b57o.js";
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
		const { webglAvailable, createScene } = await import("./scene3d-nwq9w7uj.js");
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
		onLost: () => showLight("lost")
	});
	state.view = scene;
	for (const [id, view] of state.agents) scene.setAgent(id, view, { snap: true });
}
function applyAgent(view, snap) {
	if (!view || !state.agents.has(view.agent)) return;
	const next = {
		place: view.place,
		work: view.work
	};
	state.agents.set(view.agent, next);
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
	if (!old) {
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
}
function selectConversation(key) {
	state.conversation = key;
	renderTabs();
	renderLog(true);
}
function lineElement(line) {
	const div = document.createElement("div");
	if (line.kind === "me") {
		div.className = "msg me";
		const who = document.createElement("span");
		who.className = "who";
		who.textContent = line.room === ROOM_TEAM ? `나 → ${nameOf(line.agent)}` : "나";
		div.append(who, document.createTextNode(line.text));
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
	const who = document.createElement("span");
	who.className = "who";
	who.textContent = nameOf(resp.agent);
	if (resp.status === "complete") {
		div.className = "msg agent";
		div.append(who, document.createTextNode(resp.content ?? ""));
	} else if (resp.status === "failed") {
		div.className = "msg status failed";
		div.textContent = `${nameOf(resp.agent)}: 실패 (${resp.error ?? "까닭 모름"})`;
	} else {
		div.className = "msg status";
		div.textContent = `${nameOf(resp.agent)}: ${WORK_LABEL[resp.status] ?? resp.status} …`;
	}
	return div;
}
function renderLog(forceBottom) {
	const log = $("chat-log");
	const follow = forceBottom || log.scrollHeight - log.scrollTop - log.clientHeight <= FOLLOW_PX;
	log.replaceChildren();
	for (const line of linesOf(state.conversation)) {
		const el = lineElement(line);
		if (el) log.append(el);
	}
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
		else if (frame.kind === "office.approval") applyApproval($("approvals"), frame.payload, (input, init) => fetch(input, init));
	}
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
	if (state.lastSeq !== null) url += `?last_seq=${encodeURIComponent(String(state.lastSeq))}`;
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
async function main() {
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
export { LOOKS as n, DEFAULT_LOOKS as t };
