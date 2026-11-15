//! 3D 사무실 화면(A1)의 웹 자산. 파일은 `crates/office/web/office/`에 있고 실행 파일에 담긴다.

/// 웹 자산 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebAsset {
    /// `crates/office/web/office/` 아래의 경로(`/`로 나눔, 앞에 `/` 없음). 예: `index.html`.
    pub path: &'static str,
    /// 파일 내용.
    pub body: &'static [u8],
}

/// 지도 정의 파일의 경로. 담당 6명의 책상 · 방 · 침대와 논의 탁자 자리가 있다.
///
/// 모양: `{"agents": {"<담당 id>": {"desk": {"x","y"}, "bed": {"x","y"}, "room": "<방 이름>"}}, "meeting_seats": [{"x","y"}, …]}`.
/// 좌표는 격자 칸 번호(32px 한 칸).
pub const MAP_PATH: &str = "office-map.json";

/// 화면의 첫 쪽.
pub const INDEX_PATH: &str = "index.html";

/// 웹 자산 전부. three.js(라이선스 파일 함께), 지도 정의, 화면 JS · HTML.
/// 바깥 `http(s)://`에서 아무것도 불러오지 않는다.
pub fn web_assets() -> &'static [WebAsset] {
    ASSETS
}

/// 파일 하나를 `crates/office/web/office/` 아래 경로로 담는다.
macro_rules! asset {
    ($path:literal) => {
        WebAsset {
            path: $path,
            body: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/web/office/", $path)),
        }
    };
}

/// 실행 파일에 담는 자산 목록.
///
/// three.js는 npm 패키지 `three` 0.170.0(r170)의 `build/three.module.min.js`와 `LICENSE`(MIT)를
/// 그대로 담았다. 받은 tarball은 npm 레지스트리의 `dist.integrity`(sha512)와 대조했다.
static ASSETS: &[WebAsset] = &[
    asset!("index.html"),
    asset!("office.css"),
    asset!("office.js"),
    asset!("scene3d.js"),
    asset!("nav.js"),
    asset!("office-map.json"),
    asset!("vendor/three.module.min.js"),
    asset!("vendor/three-LICENSE.txt"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 첫_쪽과_지도가_목록에_있다() {
        for path in [INDEX_PATH, MAP_PATH] {
            assert!(web_assets().iter().any(|a| a.path == path), "{path}");
        }
    }

    #[test]
    fn 화면_js가_부르는_상대_경로의_파일이_모두_있다() {
        let have: Vec<&str> = web_assets().iter().map(|a| a.path).collect();
        for a in web_assets()
            .iter()
            .filter(|a| a.path.ends_with(".js") && !a.path.starts_with("vendor/"))
        {
            let text = std::str::from_utf8(a.body).unwrap();
            for part in text.split("\"./").skip(1) {
                let Some(end) = part.find('"') else { continue };
                let target = &part[..end];
                assert!(have.contains(&target), "{} → {target}", a.path);
            }
        }
    }
}
