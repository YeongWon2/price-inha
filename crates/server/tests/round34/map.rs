//! 32차 1번 · 3번: 서버가 자리 범위를 재는 사무실 칸 수와 기본 자리(입구)가 화면 지도(`web/public/office/office-map.json`)와
//! 같다. 입구 칸은 벽 · 막는 가구가 없는 칸이다.

use price_inha::{OFFICE_COLS, OFFICE_ENTRANCE, OFFICE_ENTRANCE_DIR, OFFICE_ROWS};
use serde_json::Value;

fn office_map() -> Value {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/public/office/office-map.json");
    let text = std::fs::read_to_string(&path).expect("화면 지도를 읽는다");
    serde_json::from_str(&text).expect("화면 지도는 JSON이다")
}

fn covers(rect: &Value, x: u64, y: u64) -> bool {
    let n = |k: &str| rect[k].as_u64().expect("칸 사각형의 수");
    n("x") <= x && x < n("x") + n("w") && n("y") <= y && y < n("y") + n("h")
}

// 검증: V56 C17 M8
#[test]
fn 사무실_칸_수는_화면_지도와_같다() {
    let map = office_map();
    assert_eq!(map["cols"], OFFICE_COLS);
    assert_eq!(map["rows"], OFFICE_ROWS);
}

// 검증: V56 C17 M8
#[test]
fn 기본_자리는_화면_지도_입구의_가운데_열_한_줄_안쪽이고_걸을_수_있다() {
    let map = office_map();
    let entrance = &map["entrance"];
    let from = entrance["fromCol"].as_u64().unwrap();
    let to = entrance["toCol"].as_u64().unwrap();
    let row = entrance["row"].as_u64().unwrap();
    let (x, y) = (u64::from(OFFICE_ENTRANCE.0), u64::from(OFFICE_ENTRANCE.1));
    assert_eq!(x, (from + to) / 2, "입구 가운데 열");
    assert_eq!(y + 1, row, "입구 줄 바로 안쪽");
    assert!(x < u64::from(OFFICE_COLS) && y < u64::from(OFFICE_ROWS));
    let walls = map["walls"].as_array().unwrap();
    assert!(
        !walls.iter().any(|w| covers(w, x, y)),
        "입구 칸에 벽이 없다"
    );
    let furniture = map["furniture"].as_array().unwrap();
    assert!(
        !furniture
            .iter()
            .any(|f| f["blocks"] == true && covers(f, x, y)),
        "입구 칸에 막는 가구가 없다"
    );
    assert_eq!(OFFICE_ENTRANCE_DIR, "up", "입구에서는 사무실 안쪽을 본다");
}
