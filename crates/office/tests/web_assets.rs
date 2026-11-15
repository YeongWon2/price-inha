//! 웹 자산의 성질(결정 2026-11-16 27번: JS 시험 도구 없이 Rust 테스트로 본다).
//! 화면이 실제로 그려지는지는 G3에서 브라우저로 본다.

use std::collections::HashSet;

use pi_office::{Agent, INDEX_PATH, MAP_PATH, WebAsset, web_assets};
use serde_json::Value;

fn text(asset: &WebAsset) -> &str {
    std::str::from_utf8(asset.body).unwrap_or("")
}

fn find(path: &str) -> &'static WebAsset {
    web_assets()
        .iter()
        .find(|a| a.path == path)
        .unwrap_or_else(|| panic!("{path} 자산이 없다"))
}

fn is_code(path: &str) -> bool {
    [".html", ".js", ".mjs", ".css", ".json"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

/// 글에서 바깥 `http(s)://`를 **불러오는** 곳을 찾는다.
/// 주석 · 이름공간 글자(`http://www.w3.org/...`) 같은 단순 문자열은 불러오기가 아니므로 보지 않고,
/// 불러오는 문법 바로 뒤에 **따옴표로 감싼** 바깥 주소가 오는지만 본다
/// (`from "https://…"`, `import('https://…')`, `fetch("https://…")` 등. 주석의 "from https://…"는 아니다).
/// HTML · CSS는 바깥 주소가 아예 없어야 한다.
fn external_loads(path: &str, body: &str) -> Vec<String> {
    let mut found = Vec::new();
    let strict = path.ends_with(".html") || path.ends_with(".css");
    let loaders = [
        "from", "import(", "import", "src=", "href=", "fetch(", "url(", "@import", "Worker(",
    ];
    let is_quote = |c: char| c == '"' || c == '\'' || c == '`';
    for scheme in ["http://", "https://", "ws://", "wss://", "//cdn"] {
        let mut start = 0;
        while let Some(pos) = body[start..].find(scheme) {
            let at = start + pos;
            // 주소 바로 앞의 따옴표 · 공백(뒤에서부터)
            let gap: Vec<char> = body[..at]
                .chars()
                .rev()
                .take_while(|c| is_quote(*c) || c.is_whitespace())
                .collect();
            let quoted = gap.iter().any(|c| is_quote(*c));
            // 그 앞의 12글자(공백 빼고, 뒤에서부터 모은 뒤 뒤집는다)
            let mut before: Vec<char> = body[..at]
                .chars()
                .rev()
                .skip(gap.len())
                .filter(|c| !c.is_whitespace())
                .take(12)
                .collect();
            before.reverse();
            let compact: String = before.into_iter().collect();
            let is_loader = quoted && loaders.iter().any(|l| compact.ends_with(l));
            if strict || is_loader {
                let line = body[at..].lines().next().unwrap_or("");
                found.push(format!(
                    "{path}: {}",
                    line.chars().take(80).collect::<String>()
                ));
            }
            start = at + scheme.len();
        }
    }
    found
}

// 검증: V1, C2
#[test]
fn 자산이_있고_첫_쪽은_모듈_스크립트로_연다() {
    let assets = web_assets();
    assert!(!assets.is_empty(), "웹 자산이 없다");
    let mut seen = HashSet::new();
    for a in assets {
        assert!(seen.insert(a.path), "경로가 겹친다: {}", a.path);
        assert!(!a.path.starts_with('/'), "앞에 / 없음: {}", a.path);
        assert!(!a.path.contains(".."), "위로 가는 경로 없음: {}", a.path);
        assert!(!a.path.contains('\\'), "경로는 /로 나눔: {}", a.path);
        assert!(!a.body.is_empty(), "빈 파일: {}", a.path);
    }
    let index = text(find(INDEX_PATH));
    assert!(
        index.contains("type=\"module\"") || index.contains("type='module'"),
        "첫 쪽은 ES 모듈 스크립트를 쓴다"
    );
}

// 검증: C2
#[test]
fn 바깥_주소에서_아무것도_불러오지_않는다() {
    // 검사기 자체가 불러오기를 잡는지 먼저 본다
    assert!(
        !external_loads(
            "a.js",
            "import * as T from \"https://cdn.example/three.js\";"
        )
        .is_empty()
    );
    assert!(
        !external_loads("a.js", "const m = await import('http://cdn.example/x.js');").is_empty()
    );
    assert!(!external_loads("a.js", "fetch(\"https://cdn.example/map.json\")").is_empty());
    assert!(
        !external_loads(
            "a.html",
            "<script src=\"https://cdn.example/t.js\"></script>"
        )
        .is_empty()
    );
    assert!(!external_loads("a.css", "@import url(https://fonts.example/f.css);").is_empty());
    assert!(
        external_loads(
            "a.js",
            "document.createElementNS('http://www.w3.org/1999/xhtml', 'img')"
        )
        .is_empty(),
        "이름공간 글자는 불러오기가 아니다"
    );
    assert!(external_loads("a.js", "import * as T from './vendor/three.module.js';").is_empty());
    assert!(
        external_loads("a.js", "// adapted from https://example.org/paper.pdf").is_empty(),
        "주석의 주소는 불러오기가 아니다"
    );
    assert!(!external_loads("a.js", "new Worker( 'https://cdn.example/w.js' )").is_empty());

    let assets = web_assets();
    assert!(
        assets.iter().any(|a| a.path.ends_with(".js")),
        "JS 자산이 없다"
    );
    let mut bad = Vec::new();
    for a in assets.iter().filter(|a| is_code(a.path)) {
        bad.extend(external_loads(a.path, text(a)));
    }
    assert!(bad.is_empty(), "바깥에서 불러오는 곳:\n{}", bad.join("\n"));
}

// 검증: V1
#[test]
fn three_js가_라이선스_파일과_함께_있다() {
    let assets = web_assets();
    let three = assets.iter().find(|a| {
        let p = a.path.to_ascii_lowercase();
        p.contains("three") && (p.ends_with(".module.js") || p.ends_with(".module.min.js"))
    });
    let three = three.expect("three.js ES 모듈 파일이 없다");
    assert!(text(three).len() > 100_000, "three.js 전체가 들어 있다");

    let license = assets.iter().find(|a| {
        let p = a.path.to_ascii_lowercase();
        p.contains("three") && p.contains("license")
    });
    let license = text(license.expect("three.js 라이선스 파일이 없다"));
    assert!(license.contains("MIT License"), "MIT 라이선스 글");
    assert!(license.contains("three.js authors"), "저작권 줄");
    assert!(
        license.contains("Permission is hereby granted"),
        "허락 글 전문"
    );
}

fn point(v: &Value, what: &str) -> (i64, i64) {
    let x = v["x"].as_i64();
    let y = v["y"].as_i64();
    match (x, y) {
        (Some(x), Some(y)) if x >= 0 && y >= 0 => (x, y),
        _ => panic!("{what}: x · y가 0 이상의 정수 칸 번호여야 한다: {v}"),
    }
}

// 검증: V1, V15, V13
#[test]
fn 지도에_여섯_명_모두_책상과_침대가_있다() {
    let map: Value = serde_json::from_slice(find(MAP_PATH).body).expect("지도는 JSON이다");
    let agents = map["agents"].as_object().expect("agents 객체");
    let mut desks = HashSet::new();
    let mut beds = HashSet::new();
    let mut rooms = HashSet::new();
    for agent in Agent::ALL {
        let a = agents
            .get(agent.id())
            .unwrap_or_else(|| panic!("{} 담당이 지도에 없다", agent.id()));
        let desk = point(&a["desk"], &format!("{} 책상", agent.id()));
        let bed = point(&a["bed"], &format!("{} 침대", agent.id()));
        assert_ne!(desk, bed, "{} 책상과 침대는 다른 곳", agent.id());
        assert!(desks.insert(desk), "책상이 겹친다: {}", agent.id());
        assert!(beds.insert(bed), "침대가 겹친다: {}", agent.id());
        let room = a["room"].as_str().unwrap_or("");
        assert!(!room.trim().is_empty(), "{} 방 이름", agent.id());
        assert!(rooms.insert(room.to_owned()), "방은 각자: {}", agent.id());
    }
    assert_eq!(agents.len(), 6, "모르는 담당이 없다");

    let seats = map["meeting_seats"]
        .as_array()
        .expect("논의 탁자 자리 목록");
    assert!(seats.len() >= 6, "논의 탁자에 6명이 앉는다");
    let mut seen = HashSet::new();
    for (i, s) in seats.iter().enumerate() {
        let p = point(s, &format!("논의 자리 {i}"));
        assert!(seen.insert(p), "논의 자리가 겹친다");
        assert!(
            !desks.contains(&p) && !beds.contains(&p),
            "논의 자리는 책상 · 침대와 다른 곳"
        );
    }
}
