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
    todo!()
}
