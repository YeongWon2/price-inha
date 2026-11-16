//! 오래 켜 둔 탭의 응답 목록 정리(office 2차 후보 2). 화면이 실제로 도는 것은 G3에서 본다.

use pi_office::web_assets;

fn office_js() -> &'static str {
    let asset = web_assets()
        .iter()
        .find(|a| a.path.ends_with("office.js"))
        .expect("office.js 자산");
    std::str::from_utf8(asset.body).unwrap()
}

// 검증: C5
#[test]
fn 줄을_넘겨_밀려난_끝난_응답은_응답_표에서도_지운다() {
    let js = office_js();
    let trim = js
        .split("function trim(")
        .nth(1)
        .expect("trim 함수")
        .split("\nfunction ")
        .next()
        .unwrap();
    assert!(
        trim.contains("state.responses.delete("),
        "밀려난 줄의 응답을 응답 표에서 지운다"
    );
    assert!(
        trim.contains("complete") && trim.contains("failed"),
        "끝난 응답(complete · failed)만 지운다. 아직 오는 중인 응답은 남긴다"
    );
}
