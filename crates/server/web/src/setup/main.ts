// 설정 화면(`/`): 시세 · 이벤트 · 설정(키 · 종목 · AI · 백업 · 접속 키)을 같은 출처로만 읽고 보낸다.
// 옛 한 장 화면(web/index.html)의 인라인 스크립트를 그대로 옮긴 모듈이다(server 11차 — 하는 일 · 보내는 요청 ·
// 화면 글은 같다). 서버가 준 글자는 언제나 textContent로만 넣는다.
import "./setup.css";
import { API } from "../paths";
import { applyKeyFieldRules, submitKeyFields } from "../keys/field";

/** 서버가 보낸 JSON 또는 그 조각. 모양은 쓰는 자리에서 확인한다(옛 화면과 같은 확인). */
type Data = any;

/** 이 화면의 요소. 칸 · 단추 · 글 자리를 같은 모양(value · checked · disabled · hidden · textContent)으로 다룬다. */
function byId(id: string): HTMLInputElement {
  return document.getElementById(id) as HTMLInputElement;
}

  var SESSION_PATH = API.session;
  var WS_PATH = API.socket;
  var SETUP_PATH = API.setup;
  var SETUP_KEYS_PATH = API.setupKeys;
  var SETUP_SYMBOLS_PATH = API.setupSymbols;
  var SETUP_AI_PATH = API.setupAi;
  var SETUP_BACKUP_PATH = API.setupBackup;
  var ACCESS_KEY_PATH = API.accessKey;
  var ACCESS_UNLOCK_PATH = API.accessUnlock;
  var ACCESS_LOG_PATH = API.accessLog;
  var SETUP_SLOTS_PATH = API.setupSlots;
  // 바깥 키 칸: item 이름 → 칸 이름(KeySlot::fields).
  var SLOT_FIELDS: Record<string, string> = { dart: "api_key", ecos: "api_key", fred: "api_key", backup_github_token: "token" };
  // 공급자 선언에서 만든 키 칸 목록(이름 · 칸 이름 · 등록 여부만 온다).
  var SETUP_PROVIDERS_PATH = API.setupProviders;
  // 공급자 이름 → 화면에 적는 말(모르는 공급자는 이름 그대로).
  var PROVIDER_LABELS: Record<string, string> = { naver_news: "네이버 뉴스 키 (뉴스 검색)" };
  // 발급한 접속 키를 화면에 보여 두는 시간. 지나면 화면(DOM)에서 지운다.
  var ACCESS_KEY_SHOW_MS = 60000;
  var RETRY_MIN_MS = 1000;
  var RETRY_MAX_MS = 10000;
  var MAX_EVENTS = 20;

  var $conn = byId("st-conn");
  var $feed = byId("st-feed");
  var $late = byId("st-late");
  var $quotes = byId("quotes");
  var $empty = byId("empty");
  var $events = byId("events");
  var $demo = byId("st-demo");
  var $setupView = byId("setup-view");
  var $mainView = byId("main-view");
  var $missing = byId("setup-missing");
  var $rejected = byId("rejected");
  var $rejectedText = byId("rejected-text");
  var $fixBrokerItem = byId("fix-broker-item");
  var $closeSetup = byId("close-setup");
  var $symbolsNow = byId("symbols-now");

  // 설정 항목 이름 → 화면에 적는 말.
  var ITEM_LABELS: Record<string, string> = { broker: "증권사 키", ai: "AI 연결", symbols: "지켜볼 종목" };
  // 등록 실패 까닭 → 화면에 적는 말.
  var REASONS: Record<string, string> = {
    "key_rejected": "키가 맞지 않습니다. 값을 확인하세요.",
    "unreachable": "확인할 서버에 닿지 않습니다. 잠시 뒤에 다시 하세요.",
    "invalid_input": "입력이 잘못됐습니다. 빈 칸이나 너무 짧은 값이 없는지 확인하세요.",
    "too_many_attempts": "시도가 너무 많습니다. 잠시 뒤에 다시 하세요.",
    "fixed_by_argument": "종목을 실행 인자로 줘서 이 화면에서는 바꿀 수 없습니다.",
    "save_failed": "저장하지 못했습니다. 보관 폴더를 확인하세요.",
    "cli_not_installed": "그 경로에 Claude Code CLI가 없습니다. 실행 파일 경로를 확인하세요.",
    "cli_too_old": "Claude Code CLI 버전이 너무 낮습니다. 업데이트한 뒤 다시 확인하세요.",
    "cli_not_logged_in": "로그인돼 있지 않습니다. 터미널에서 claude auth login 을 실행한 뒤 다시 확인하세요.",
    "cli_not_subscription": "구독으로 로그인돼 있지 않습니다(API 키로 로그인돼 있습니다). 구독 계정으로 다시 로그인하세요.",
    "cli_unchecked": "확인하지 못했습니다(시간 초과 · 읽지 못함). 잠시 뒤에 다시 확인하세요.",
    "cli_work_dir_not_empty": "보관 폴더의 cli-work 폴더가 비어 있지 않아 CLI를 쓰지 않습니다. 안의 것을 옮기거나 지운 뒤 다시 확인하세요.",
    "shutting_down": "서버가 꺼지는 중입니다. 다시 켠 뒤에 고르세요."
  };

  // 마지막 CLI 확인 결과(설정 상태의 ai.cli)를 글자로.
  var CLI_STATES: Record<string, string> = {
    "logged_in": "구독으로 로그인돼 있습니다.",
    "not_installed": "설치되지 않았습니다(그 경로에 실행 파일이 없습니다).",
    "too_old": "버전이 너무 낮습니다. 업데이트하세요.",
    "not_logged_in": "로그인돼 있지 않습니다. 터미널에서 claude auth login 을 실행하세요.",
    "not_subscription": "구독이 아닙니다(API 키로 로그인돼 있습니다).",
    "unchecked": "확인하지 못했습니다(시간 초과 · 읽지 못함).",
    "checking": "확인하는 중입니다…",
    "work_dir_not_empty": "보관 폴더의 cli-work 폴더가 비어 있지 않아 쓰지 않습니다(안의 것은 지우지 않았습니다)."
  };

  var lastSeq: number | null = null;      // 마지막으로 받은 이벤트 번호. 아직 없으면 null(지금부터 받는다).
  var retryMs = RETRY_MIN_MS;
  var rows: Record<string, Data> = {};           // 종목 → { tr, price, currency, at, last }
  var lateSymbols: Record<string, boolean> = {};
  var started = false;     // 시세 받기를 시작했는가(서버가 알려 준 것).
  var demoMode = false;
  var manualSetup = false; // 시작한 뒤 사용자가 설정 화면을 열었는가.

  // 서버가 준 글자는 언제나 textContent로만 넣는다.
  function setText(node: Data, text: Data) { node.textContent = String(text); }

  function setConn(text: Data, kind: Data) {
    setText($conn, text);
    $conn.className = kind;
    document.body.classList.toggle("offline", kind !== "ok");
  }



  function cell(tr: Data, className?: string) {
    var td = document.createElement("td");
    if (className) { td.className = className; }
    tr.appendChild(td);
    return td;
  }

  function rowFor(symbol: Data) {
    if (rows[symbol]) { return rows[symbol]; }
    var tr = document.createElement("tr");
    var row: Data = { tr: tr, last: null };
    setText(cell(tr), symbol);
    row.price = cell(tr, "num");
    row.currency = cell(tr);
    row.at = cell(tr);
    $quotes.appendChild(tr);
    rows[symbol] = row;
    return row;
  }

  function showQuotes(payload: Data) {
    var list = payload && Array.isArray(payload.quotes) ? payload.quotes : [];
    list.forEach(function (quote: Data) {
      if (!quote || typeof quote.symbol !== "string") { return; }
      var row = rowFor(quote.symbol);
      var price = Number(quote.price);
      var direction = "";
      if (row.last !== null && isFinite(price)) {
        direction = price > row.last ? " up" : price < row.last ? " down" : "";
      }
      if (isFinite(price)) { row.last = price; }
      row.price.className = "num" + direction;
      setText(row.price, quote.price);
      setText(row.currency, quote.currency || "");
      setText(row.at, timeText(quote.received_at));
    });
    $empty.hidden = Object.keys(rows).length > 0;
  }

  function showFeed(payload: Data) {
    var state = payload ? payload.state : "";
    var text = "시세 상태를 알 수 없음";
    var kind = "warn";
    if (state === "no_broker_key") { text = "증권사 키가 없음 (시세를 받지 않습니다)"; kind = "bad"; }
    else if (state === "setup_required") { text = "필수 설정이 필요함 (시작하지 않았습니다)"; kind = "bad"; }
    else if (state === "request_rejected") { text = "증권사가 요청을 거절함 (종목을 확인하세요 · 간격을 늘려 다시 시도 중)"; kind = "bad"; }
    else if (state === "demo") { text = "시연 (가짜 시세)"; kind = "warn"; }
    else if (state === "live") { text = "증권사 시세"; kind = "ok"; }
    else if (state === "connecting") { text = "증권사에 연결 중 (아직 받은 시세가 없습니다)"; kind = "warn"; }
    else if (state === "broker_rejected") { text = "증권사가 키를 받지 않음 (키나 허용 IP를 확인하세요 · 간격을 늘려 다시 시도 중)"; kind = "bad"; }
    else if (state === "broker_unreachable") { text = "증권사에 닿지 않음 (간격을 늘려 다시 시도 중)"; kind = "bad"; }
    setText($feed, text);
    $feed.className = kind;
    $feed.hidden = false;
    // 키를 받지 않으면 옆에 고치러 가는 길을 둔다.
    $fixBrokerItem.hidden = state !== "broker_rejected";
  }

  function showLate(payload: Data) {
    var list = payload && Array.isArray(payload.symbols) ? payload.symbols : [];
    lateSymbols = {};
    list.forEach(function (symbol: Data) { lateSymbols[String(symbol)] = true; });
    Object.keys(rows).forEach(function (symbol: Data) {
      rows[symbol].tr.classList.toggle("late", lateSymbols[symbol] === true);
    });
    // 장 시간을 모르는 채로 판단한 것이면 그렇게 적는다(장이 닫혀서 안 오는 것일 수도 있다).
    var unknown = payload && payload.hours_unknown === true;
    var text = list.length > 0 ? "시세 늦음 (" + list.length + "종목)" : "시세 늦음";
    setText($late, unknown ? text + " · 장 시간을 모름" : text);
    $late.hidden = list.length === 0;
  }

  // 설정 화면과 본 화면 가운데 하나만 보여 준다. 시작한 뒤에는 사용자가 설정 화면을 열 수 있다.
  function render() {
    if (!started) { manualSetup = false; }
    var setup = !started || manualSetup;
    $setupView.hidden = !setup;
    $mainView.hidden = setup;
    $closeSetup.hidden = !started;
    $demo.hidden = !demoMode;
  }

  function showView(isStarted: Data, demo: Data) {
    started = isStarted;
    demoMode = demo;
    render();
  }

  // 설정 화면을 연다. 고칠 칸이 있으면 그 칸으로 간다.
  function openSetup(focusId: Data) {
    manualSetup = true;
    render();
    loadSetup();
    var field = focusId ? byId(focusId) : null;
    if (field) { field.focus(); }
  }

  function closeSetup() {
    manualSetup = false;
    render();
  }

  function missingText(names: Data) {
    var list = Array.isArray(names) ? names : [];
    if (list.length === 0) { return ""; }
    var words = list.map(function (name: Data) { return ITEM_LABELS[name] || String(name); });
    return "아직 등록되지 않은 필수 항목: " + words.join(", ");
  }

  // 설정 상태(등록 여부만 온다)를 그린다.
  function showSetupStatus(status: Data) {
    if (!status || !Array.isArray(status.items)) { return; }
    var missing: string[] = [];
    status.items.forEach(function (item: Data) {
      if (!item || typeof item.name !== "string") { return; }
      var badge = byId("reg-" + item.name);
      if (badge) {
        setText(badge, item.registered === true ? "등록됨" : "등록 안 됨");
        badge.className = "badge " + (item.registered === true ? "ok" : "warn");
      }
      if (item.required === true && item.registered !== true) { missing.push(item.name); }
    });
    setText($missing, missingText(missing));
    showAiStatus(status.ai);
    var symbols = Array.isArray(status.symbols) ? status.symbols : [];
    var fixed = status.symbols_source === "argument";
    setText($symbolsNow, symbols.length === 0 ? "" :
      "지금 종목: " + symbols.join(", ") + (fixed ? " (실행 인자로 줘서 여기서는 바꿀 수 없습니다)" : ""));
    showView(status.started === true, status.demo === true);
  }

  function loadSetup() {
    fetch(SETUP_PATH, { credentials: "same-origin", cache: "no-store" })
      .then(function (reply: Data) { return reply.ok ? reply.json() : null; })
      .then(showSetupStatus)
      .catch(function () { /* 다음 설정 알림이나 다시 이을 때 다시 읽는다 */ });
    loadSlots();
  }

  // 바깥 키 칸별 등록 여부(이름과 등록 여부만 온다).
  function loadSlots() {
    fetch(SETUP_SLOTS_PATH, { credentials: "same-origin", cache: "no-store" })
      .then(function (reply: Data) { return reply.ok ? reply.json() : null; })
      .then(function (view: Data) {
        if (!view || !Array.isArray(view.slots)) { return; }
        view.slots.forEach(function (slot: Data) {
          var badge = slot && byId("reg-slot-" + slot.name);
          if (!badge) { return; }
          setText(badge, slot.registered === true ? "등록됨" : "등록 안 됨");
          badge.className = "badge " + (slot.registered === true ? "ok" : "warn");
        });
      })
      .catch(function () { /* 다음 설정 읽기에서 다시 읽는다 */ });
    loadProviders();
  }

  // 공급자 칸 하나(칸마다 비밀 입력 칸 · 등록 단추 · 알림 자리)를 만들고 기존 키 등록 길에 잇는다.
  function providerBlock(provider: Data) {
    var name = String(provider.provider);
    var $block = document.createElement("div");
    $block.className = "slot";
    $block.id = "provider-" + name;
    var $title = document.createElement("h4");
    setText($title, (PROVIDER_LABELS[name] || name) + " ");
    var $badge = document.createElement("span");
    $badge.id = "reg-provider-" + name;
    $badge.className = "badge";
    $title.appendChild($badge);
    $block.appendChild($title);
    var fields: Record<string, string> = {};
    provider.fields.forEach(function (field: Data) {
      var id = "f-provider-" + name + "-" + String(field);
      var $label = document.createElement("label");
      $label.htmlFor = id;
      setText($label, String(field));
      var $input = document.createElement("input");
      $input.id = id;
      applyKeyFieldRules($input);
      $block.appendChild($label);
      $block.appendChild($input);
      fields[String(field)] = id;
    });
    var $button = document.createElement("button");
    $button.id = "b-provider-" + name;
    $button.type = "button";
    setText($button, "등록");
    var $message = document.createElement("span");
    $message.id = "m-provider-" + name;
    $message.className = "msg";
    $message.setAttribute("aria-live", "polite");
    $block.appendChild($button);
    $block.appendChild($message);
    byId("provider-slots").appendChild($block);
    wireKey($button.id, $message.id, name, fields);
  }

  // 공급자 칸 목록을 읽어 없는 칸은 그리고 등록 여부를 적는다(값은 오지 않는다).
  function loadProviders() {
    fetch(SETUP_PROVIDERS_PATH, { credentials: "same-origin", cache: "no-store" })
      .then(function (reply: Data) { return reply.ok ? reply.json() : null; })
      .then(function (view: Data) {
        if (!view || !Array.isArray(view.providers)) { return; }
        view.providers.forEach(function (provider: Data) {
          if (!provider || typeof provider.provider !== "string" || !Array.isArray(provider.fields)) { return; }
          if (!byId("provider-" + provider.provider)) { providerBlock(provider); }
          var badge = byId("reg-provider-" + provider.provider);
          setText(badge, provider.registered === true ? "등록됨" : "등록 안 됨");
          badge.className = "badge " + (provider.registered === true ? "ok" : "warn");
        });
      })
      .catch(function () { /* 다음 설정 읽기에서 다시 읽는다 */ });
  }

  // 설정 알림: 시작했으면 본 화면으로, 필수 항목이 빠졌으면 설정 화면으로.
  function showSetup(payload: Data) {
    if (!payload) { return; }
    if (payload.state === "started") { showView(true, payload.demo === true); }
    else if (payload.state === "required") {
      setText($missing, missingText(payload.missing));
      showView(false, payload.demo === true);
    }
    // 항목별 등록 여부는 설정 상태에서 다시 읽는다.
    loadSetup();
  }

  function showRejected(payload: Data) {
    var list = payload && Array.isArray(payload.symbols) ? payload.symbols : [];
    setText($rejectedText, list.length === 0 ? "" :
      "증권사가 거절한 종목 (빼고 받는 중 · 종목을 확인하세요): " + list.join(", "));
    $rejected.hidden = list.length === 0;
  }

  // 등록 요청을 보낸다. 값은 본문으로만 나가고 어디에도 남기지 않는다. 받아들여졌는지(`true`)를 알려 준다.
  // 칸을 비우는 일은 부른 쪽이 한다(키 칸은 `submitKeyFields`가, 종목 입력은 아래 단추가).
  function register(path: Data, body: Data, $message: Data, $button: Data): Promise<boolean> {
    var accepted = false;
    $button.disabled = true;
    setText($message, "확인하는 중…");
    $message.className = "msg";
    return fetch(path, {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body)
    })
      .then(function (reply: Data) {
        return reply.json().catch(function () { return null; }).then(function (data: Data) {
          return { ok: reply.ok, status: reply.status, data: data };
        });
      })
      .then(function (result: Data) {
        if (result.ok && result.data && result.data.registered === true) {
          accepted = true;
          setText($message, "등록됨");
          $message.className = "msg ok";
        } else {
          var reason = result.data && typeof result.data.error === "string" ? result.data.error : "";
          var text = REASONS[reason];
          if (!text) {
            text = result.status === 401 ? "세션이 끝났습니다. 화면을 새로 고치세요."
              : result.status === 403 ? "이 컴퓨터에서 연 화면에서만 등록할 수 있습니다."
              : "등록하지 못했습니다.";
          }
          setText($message, text);
          $message.className = "msg bad";
        }
        loadSetup();
      })
      .catch(function () {
        setText($message, "서버에 닿지 않습니다.");
        $message.className = "msg bad";
      })
      .then(function () {
        $button.disabled = false;
        return accepted;
      });
  }

  // AI 연결 칸(고른 방식 · 경로 · 확인 상태 · 옛 파일 알림)을 그린다. 키 값은 오지 않는다.
  var aiPicked = false;    // 사용자가 고르기를 눌렀고 아직 서버에 보내지 않았다.
  // 사용자가 고친 경로 칸(입력 이벤트로 기억한다). 서버에 보내기 전까지 상태가 덮지 않는다.
  var cliEdited: Record<string, boolean> = { "f-cli-command": false, "f-cli-config-dir": false };
  function showAiPick(method: Data) {
    byId("ai-pick-api").checked = method !== "cli";
    byId("ai-pick-cli").checked = method === "cli";
    byId("ai-api-box").hidden = method === "cli";
    byId("ai-cli-box").hidden = method !== "cli";
  }
  function showAiStatus(ai: Data) {
    if (!ai || typeof ai !== "object") { return; }
    if (!aiPicked) { showAiPick(ai.method === "cli" ? "cli" : "api_key"); }
    var $command = byId("f-cli-command");
    if (typeof ai.command === "string" && ($command.value === "" || ai.method === "cli")
        && !cliEdited["f-cli-command"] && document.activeElement !== $command) {
      $command.value = ai.command;
    }
    var $dir = byId("f-cli-config-dir");
    if (ai.method === "cli" && !cliEdited["f-cli-config-dir"] && document.activeElement !== $dir) {
      $dir.value = typeof ai.config_dir === "string" ? ai.config_dir : "";
    }
    var line = "";
    if (ai.method === "cli" && typeof ai.cli === "string") {
      line = "CLI 상태: " + (CLI_STATES[ai.cli] || "알 수 없음");
      if (ai.cli === "logged_in" && typeof ai.cli_version === "string") {
        line += " (버전 " + ai.cli_version +
          (typeof ai.subscription === "string" ? ", 구독 " + ai.subscription : "") + ")";
      }
    }
    setText(byId("ai-cli-state"), line);
    byId("ai-legacy").hidden = ai.legacy_auth_file !== true;
  }

  // AI 연결을 고른다(CLI면 서버가 확인한다). 갖춰졌는지나 까닭만 돌아온다.
  function chooseAi(body: Data, $button: Data) {
    var $message = byId("m-ai");
    if ($button) { $button.disabled = true; }
    setText($message, body.method === "cli" ? "확인하는 중…" : "저장하는 중…");
    $message.className = "msg";
    fetch(SETUP_AI_PATH, {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body)
    })
      .then(function (reply: Data) {
        return reply.json().catch(function () { return null; }).then(function (data: Data) {
          return { ok: reply.ok, status: reply.status, data: data };
        });
      })
      .then(function (result: Data) {
        var data = result.data;
        if (result.ok && data && data.registered === true) {
          setText($message, "AI 연결이 갖춰졌습니다");
          $message.className = "msg ok";
        } else if (result.ok && data && data.registered === false) {
          setText($message, "API 키를 골랐습니다. 아래에 API 키를 등록하세요.");
          $message.className = "msg warn";
        } else {
          var reason = data && typeof data.error === "string" ? data.error : "";
          setText($message, REASONS[reason] || (result.status === 401 ? "세션이 끝났습니다. 화면을 새로 고치세요."
            : result.status === 403 ? "이 컴퓨터에서 연 화면에서만 고를 수 있습니다." : "고르지 못했습니다."));
          $message.className = "msg bad";
        }
        aiPicked = false;
        // 서버가 고른 것을 저장했을 때만(성공 · CLI 확인 결과) 칸을 상태에 다시 맞춘다.
        // 입력이 잘못됐거나 횟수를 넘었으면 사용자가 고치던 글자를 남긴다.
        var why = data && typeof data.error === "string" ? data.error : "";
        if (result.ok || why.indexOf("cli_") === 0) {
          cliEdited["f-cli-command"] = false;
          cliEdited["f-cli-config-dir"] = false;
        }
        loadSetup();
      })
      .catch(function () {
        setText($message, "서버에 닿지 않습니다.");
        $message.className = "msg bad";
      })
      .then(function () { if ($button) { $button.disabled = false; } });
  }

  function wireAi() {
    byId("ai-pick-api").addEventListener("change", function () {
      aiPicked = true;
      showAiPick("api_key");
      chooseAi({ "method": "api_key" }, null);
    });
    byId("ai-pick-cli").addEventListener("change", function () {
      // CLI는 "확인"을 눌러야 저장하고 확인한다.
      aiPicked = true;
      showAiPick("cli");
    });
    Object.keys(cliEdited).forEach(function (id: Data) {
      byId(id).addEventListener("input", function () { cliEdited[id] = true; });
    });
    var $button = byId("b-ai-cli");
    $button.addEventListener("click", function () {
      var dir = byId("f-cli-config-dir").value.trim();
      chooseAi({
        "method": "cli",
        "command": byId("f-cli-command").value.trim(),
        "config_dir": dir === "" ? null : dir
      }, $button);
    });
  }

  // 키 항목 하나의 단추를 잇는다: fields는 { 칸 이름: 입력 칸 id }.
  function wireKey(buttonId: Data, messageId: Data, item: Data, fields: Data) {
    var $button = byId(buttonId);
    var $message = byId(messageId);
    $button.addEventListener("click", function () {
      var inputs: Record<string, HTMLInputElement> = {};
      Object.keys(fields).forEach(function (name: Data) {
        inputs[name] = byId(fields[name]);
      });
      // 받아들여졌을 때만 칸을 비운다(키 화면과 같은 규칙).
      void submitKeyFields(inputs, function (raw) {
        var values: Record<string, string> = {};
        Object.keys(raw).forEach(function (name) { values[name] = raw[name].trim(); });
        return register(SETUP_KEYS_PATH, { item: item, fields: values }, $message, $button);
      });
    });
  }

  function showBackup(view: Data) {
    var $repo = byId("f-backup-repo");
    var $recipient = byId("f-backup-recipient");
    if (view && view.configured) {
      $repo.value = view.repo || "";
      $recipient.value = view.recipient || "";
    }
    setText(byId("reg-backup"), view && view.configured ? "설정됨" : "설정 안 됨");
  }

  function loadBackup() {
    fetch(SETUP_BACKUP_PATH, { credentials: "same-origin" })
      .then(function (reply: Data) { return reply.ok ? reply.json() : null; })
      .then(function (view: Data) { if (view) { showBackup(view); } })
      .catch(function () { /* 다음에 다시 읽는다 */ });
  }

  function wireBackup() {
    var $button = byId("b-backup");
    var $message = byId("m-backup");
    $button.addEventListener("click", function () {
      var body = {
        repo: byId("f-backup-repo").value.trim(),
        recipient: byId("f-backup-recipient").value.trim()
      };
      $button.disabled = true;
      fetch(SETUP_BACKUP_PATH, {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body)
      })
        .then(function (reply: Data) {
          return reply.json().catch(function () { return null; }).then(function (data: Data) {
            if (reply.ok && data) {
              showBackup(data);
              setText($message, "저장했습니다");
            } else {
              setText($message, "저장소 이름(소유자/이름)이나 공개키(age1…)를 확인하세요");
            }
          });
        })
        .catch(function () { setText($message, "서버에 닿지 못했습니다"); })
        .then(function () { $button.disabled = false; });
    });
    loadBackup();
  }

  // (7차) 접속 키 · 잠긴 기기 · 접속 기록. 키 글자는 발급 답에서 한 번만 보여 주고 간직하지 않는다.
  function postJson(path: Data, body: Data) {
    return fetch(path, {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body)
    });
  }

  function clearList(node: Data) { while (node.firstChild) { node.removeChild(node.firstChild); } }

  function timeText(ms: Data) { return new Date(ms).toLocaleString(); }

  function showAccess(view: Data) {
    var $locked = byId("access-locked");
    var $log = byId("access-log");
    clearList($locked);
    clearList($log);
    (view.locked || []).forEach(function (device: Data) {
      var $item = document.createElement("li");
      var $unlock = document.createElement("button");
      $unlock.type = "button";
      $unlock.textContent = "풀기";
      $unlock.addEventListener("click", function () {
        $unlock.disabled = true;
        postJson(ACCESS_UNLOCK_PATH, { device: device.device })
          .then(function () { loadAccess(); })
          .catch(function () { $unlock.disabled = false; });
      });
      $item.appendChild(document.createTextNode(device.device + " · " + timeText(device.until) + "까지 "));
      $item.appendChild($unlock);
      $locked.appendChild($item);
    });
    if (!view.locked || view.locked.length === 0) {
      var $none = document.createElement("li");
      $none.textContent = "없음";
      $locked.appendChild($none);
    }
    (view.entries || []).forEach(function (entry: Data) {
      var $item = document.createElement("li");
      $item.textContent = timeText(entry.at) + " · " + entry.origin + " · " + entry.action;
      $log.appendChild($item);
    });
  }

  function loadAccess() {
    fetch(ACCESS_LOG_PATH, { credentials: "same-origin", cache: "no-store" })
      .then(function (reply: Data) { return reply.ok ? reply.json() : null; })
      .then(function (view: Data) { if (view) { showAccess(view); } })
      .catch(function () { /* 다시 읽기로 다시 읽는다 */ });
  }

  function wireAccess() {
    var $button = byId("b-access-key");
    var $message = byId("m-access-key");
    var $once = byId("access-key-once");
    var $value = byId("access-key-value");
    var hideTimer: ReturnType<typeof setTimeout> | null = null;
    function hideAccessKey() {
      if (hideTimer !== null) { clearTimeout(hideTimer); hideTimer = null; }
      $once.hidden = true;
      setText($value, "");
    }
    $button.addEventListener("click", function () {
      $button.disabled = true;
      hideAccessKey();
      postJson(ACCESS_KEY_PATH, {})
        .then(function (reply: Data) {
          return reply.json().catch(function () { return null; }).then(function (data: Data) {
            if (reply.ok && data && data.access_key) {
              setText($value, data.access_key);
              $once.hidden = false;
              hideTimer = setTimeout(hideAccessKey, ACCESS_KEY_SHOW_MS);
              setText($message, "만들었습니다 (1분 뒤 화면에서 지웁니다)");
              loadAccess();
            } else if (reply.status === 403) {
              setText($message, "이 컴퓨터에서만 만들 수 있습니다");
            } else {
              setText($message, "만들지 못했습니다");
            }
          });
        })
        .catch(function () { setText($message, "서버에 닿지 못했습니다"); })
        .then(function () { $button.disabled = false; });
    });
    byId("b-access-log").addEventListener("click", loadAccess);
    loadAccess();
  }

  function wireSetup() {
    wireBackup();
    wireAccess();
    wireKey("b-broker", "m-broker", "broker",
      { client_id: "f-client-id", client_secret: "f-client-secret" });
    wireKey("b-ai-key", "m-ai-key", "ai_api_key", { api_key: "f-api-key" });
    wireAi();
    Object.keys(SLOT_FIELDS).forEach(function (item: Data) {
      var fields: Record<string, string> = {};
      fields[SLOT_FIELDS[item]] = "f-slot-" + item;
      wireKey("b-slot-" + item, "m-slot-" + item, item, fields);
    });
    byId("open-setup").addEventListener("click", function () { openSetup(null); });
    byId("close-setup").addEventListener("click", closeSetup);
    byId("fix-symbols").addEventListener("click", function () { openSetup("f-symbols"); });
    byId("fix-broker").addEventListener("click", function () { openSetup("f-client-id"); });
    var $button = byId("b-symbols");
    var $message = byId("m-symbols");
    var $input = byId("f-symbols");
    $button.addEventListener("click", function () {
      var symbols = $input.value.split(",")
        .map(function (code: Data) { return code.trim(); })
        .filter(function (code: Data) { return code.length > 0; });
      register(SETUP_SYMBOLS_PATH, { symbols: symbols }, $message, $button).then(function (accepted: boolean) {
        if (accepted) { $input.value = ""; }
      });
    });
  }

  function showEvent(frame: Data) {
    var li = document.createElement("li");
    setText(li, "#" + frame.seq + " · " + timeText(frame.at) + " · " + frame.kind);
    $events.insertBefore(li, $events.firstChild);
    while ($events.childNodes.length > MAX_EVENTS) { $events.removeChild($events.lastChild as ChildNode); }
  }

  function onFrame(text: Data) {
    var frame;
    try { frame = JSON.parse(text); } catch (error) { return; }
    if (!frame || typeof frame.kind !== "string") { return; }
    if (typeof frame.seq === "number") {
      // 저장되는 이벤트: 이미 받은 번호는 다시 그리지 않는다.
      if (lastSeq !== null && frame.seq <= lastSeq) { return; }
      lastSeq = frame.seq;
      showEvent(frame);
      return;
    }
    // 지나가는 이벤트(번호 없음).
    if (frame.kind === "quotes") { showQuotes(frame.payload); }
    else if (frame.kind === "feed_status") { showFeed(frame.payload); }
    else if (frame.kind === "quotes_stale") { showLate(frame.payload); }
    else if (frame.kind === "setup") { showSetup(frame.payload); }
    else if (frame.kind === "symbols_rejected") { showRejected(frame.payload); }
  }

  function retryLater() {
    var wait = retryMs;
    retryMs = Math.min(retryMs * 2, RETRY_MAX_MS);
    setTimeout(start, wait);
  }

  function openSocket() {
    var scheme = location.protocol === "https:" ? "wss://" : "ws://";
    var url = scheme + location.host + WS_PATH;
    // 끊겼다가 다시 이을 때는 마지막으로 받은 번호 뒤부터 받는다.
    if (lastSeq !== null) { url += "?last_seq=" + encodeURIComponent(String(lastSeq)); }
    var socket: WebSocket;
    try { socket = new WebSocket(url); } catch (error) { retryLater(); return; }
    var closed = false;
    function onClose() {
      if (closed) { return; }
      closed = true;
      setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
      retryLater();
    }
    socket.onopen = function () {
      retryMs = RETRY_MIN_MS;
      setConn("연결됨", "ok");
    };
    socket.onmessage = function (event: Data) {
      if (typeof event.data === "string") { onFrame(event.data); }
    };
    socket.onclose = onClose;
    socket.onerror = function () { try { socket.close(); } catch (error) { /* 이미 닫힘 */ } onClose(); };
  }

  // 세션을 얻고(서버를 다시 켜면 세션이 사라지므로 이을 때마다 얻는다) 실시간 연결을 연다.
  function start() {
    fetch(SESSION_PATH, { method: "POST", credentials: "same-origin", cache: "no-store" })
      .then(function (reply: Data) {
        if (!reply.ok) { throw new Error("session"); }
        loadSetup();
        openSocket();
      })
      .catch(function () {
        setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
        retryLater();
      });
  }

  wireSetup();
  start();
