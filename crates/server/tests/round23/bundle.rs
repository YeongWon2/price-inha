//! 1번 · 2번 · 5번: 사무실 3D 묶음은 외형 GLB를 같은 출처 경로로 부르고 바깥 주소가 없다.
//! 카드 화면(사무실 첫 묶음) · 폰 · 설정 · 키 묶음은 3D · 자산을 부르지 않는다(round20 dist 시험과 같은 방법:
//! dist의 묶음 글을 읽는다).

use crate::screen_built::built_page;
use crate::screen_dist::dist_dir;
use crate::screen_refs::bundle_refs;

/// GLB 외형이 놓이는 같은 출처 경로(앞부분).
const LOOK_DIR: &str = "/office/deskrpg/characters/office/";

/// 따옴표 바로 뒤에 `http(s)://`로 시작하는 글(= 코드가 부르는 바깥 주소). three 안의 `http://www.w3.org/`
/// (이름공간 글자)과 `"https:"`(프로토콜 비교)는 망을 타지 않으니 뺀다. 설명 글(주석)은 따옴표 뒤가 아니다.
fn outside_literals(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for quote in ['"', '\'', '`'] {
        for piece in body.split(quote).skip(1) {
            if piece.starts_with("http://") || piece.starts_with("https://") {
                let lit: String = piece.chars().take(48).collect();
                if !lit.starts_with("http://www.w3.org/") {
                    out.push(lit);
                }
            }
        }
    }
    out
}

/// 사무실 쪽 html 첫 묶음 글(동적으로 부르는 3D 묶음은 빼고 html이 바로 부르는 파일들만).
fn entry_text(page: &str) -> String {
    let html = std::fs::read_to_string(dist_dir().join(page))
        .unwrap_or_else(|e| panic!("dist/{page}이 있어야 한다: {e}"));
    let mut text = String::new();
    for url in bundle_refs(&html) {
        if url.ends_with(".js") {
            text.push_str(
                &std::fs::read_to_string(dist_dir().join(url.trim_start_matches('/'))).unwrap(),
            );
        }
    }
    text
}

// 검증: D1 V12 V15
#[test]
fn 사무실_3d_묶음은_외형_glb를_같은_출처_경로로_부르고_바깥_주소가_없다() {
    let office = built_page("office/index.html");
    assert!(office.contains("WebGLRenderer"), "3D 묶음이 사무실에 있다");
    assert!(
        office.contains(LOOK_DIR),
        "사무실 3D 묶음이 외형을 {LOOK_DIR}<외형>.glb 로 부른다"
    );
    assert!(office.contains(".glb"), "glb 파일 이름을 만든다");
    let outside = outside_literals(&office);
    assert!(
        outside.is_empty(),
        "사무실 묶음이 바깥 주소를 부른다: {outside:?}"
    );
    for host in ["cdn.", "githubusercontent", "unpkg.com", "jsdelivr"] {
        assert!(
            !office.contains(host),
            "사무실 묶음이 바깥 배포처 {host}를 부른다"
        );
    }
}

// 검증: V10 V12
#[test]
fn 카드_화면과_폰과_설정과_키_묶음은_glb와_원본_자산_경로를_부르지_않는다() {
    // 카드 화면: 사무실 html이 바로 부르는 첫 묶음(3D는 동적으로 늦게 부른다).
    let card = entry_text("office/index.html");
    assert!(
        card.contains("scene3d") || card.contains("import("),
        "사무실 첫 묶음이 3D를 늦게 부른다"
    );
    // 첫 묶음이 3D 쪽과 같은 대로 이 시험이 의미 있으려면 3D 묶음이 자산을 부르고 있어야 한다.
    assert!(
        built_page("office/index.html").contains(LOOK_DIR),
        "3D 묶음은 외형을 부른다(아래 검사가 뜻을 가지려면)"
    );
    let mut texts = vec![("사무실 첫 묶음".to_owned(), card)];
    for page in ["phone/index.html", "index.html", "keys.html"] {
        texts.push((format!("{page} 묶음"), built_page(page)));
    }
    for (what, body) in texts {
        assert!(!body.contains(".glb"), "{what}이 glb를 부른다");
        assert!(
            !body.contains("deskrpg"),
            "{what}이 원본 자산 경로를 부른다"
        );
        assert!(!body.contains("GLTFLoader"), "{what}에 GLTFLoader가 있다");
    }
}
