//! 1번: 원본 DeskRPG 자산(커밋 5d80963)이 `web/public/office/deskrpg/`와 `web/dist/office/deskrpg/`에
//! 해시 표와 같은 바이트로 있다. `web/public`은 Vite가 `dist`로 그대로 옮기는 곳이다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::common::{Bench, get, table};
use crate::screen_dist::dist_dir;

/// 들인 폴더(둘 다): `web/public/office/deskrpg` · `web/dist/office/deskrpg`.
fn roots() -> [(&'static str, PathBuf); 2] {
    [
        (
            "web/public",
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/public/office/deskrpg"),
        ),
        ("web/dist", dist_dir().join("office/deskrpg")),
    ]
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 폴더 아래 모든 파일의 상대 경로(`/` 구분).
fn files_under(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("폴더 항목").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("폴더 아래")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(rel);
            }
        }
    }
    out
}

// 검증: D1 V15
#[test]
fn 원본_자산이_public과_dist에_표와_같은_바이트로_들어_있다() {
    // 표 자체가 외형 50개 · 라이선스 · 세 폴더를 담고 brand · eunchae-mpfb는 없다.
    let rows = table();
    let looks = rows
        .iter()
        .filter(|(rel, _)| rel.starts_with("characters/office/") && rel.ends_with(".glb"))
        .count();
    assert_eq!(looks, 50, "외형 GLB 50개");
    for must in ["characters/LICENSES.md", "LICENSE.md"] {
        assert!(
            rows.iter().any(|(rel, _)| rel == must),
            "{must}가 표에 있다"
        );
    }
    for top in ["environments/", "furniture/", "shared/"] {
        assert!(
            rows.iter().any(|(rel, _)| rel.starts_with(top)),
            "{top} 아래 파일이 표에 있다"
        );
    }
    for (rel, _) in &rows {
        assert!(!rel.starts_with("brand/"), "brand/는 들이지 않는다: {rel}");
        assert!(
            !rel.contains("eunchae-mpfb"),
            "eunchae-mpfb는 들이지 않는다: {rel}"
        );
    }
    // 표의 파일마다 같은 바이트.
    for (name, root) in roots() {
        let mut bad: Vec<String> = Vec::new();
        for (rel, hash) in table() {
            match std::fs::read(root.join(&rel)) {
                Ok(bytes) if sha256_hex(&bytes) == hash => {}
                Ok(_) => bad.push(format!("{rel}(바이트가 다르다)")),
                Err(_) => bad.push(format!("{rel}(없다)")),
            }
        }
        assert!(
            bad.is_empty(),
            "{name}/office/deskrpg에 원본 그대로가 아닌 파일 {}개: {:?}",
            bad.len(),
            &bad[..bad.len().min(5)]
        );
    }
}

// 검증: D1 V15
#[test]
fn 들인_폴더에는_표의_파일과_source_md_말고_다른_것이_없고_brand와_eunchae_mpfb는_없다() {
    let mut expected: BTreeSet<String> = table().into_iter().map(|(rel, _)| rel).collect();
    expected.insert("SOURCE.md".to_owned());
    for (name, root) in roots() {
        assert!(root.is_dir(), "{name}/office/deskrpg 폴더가 있다");
        assert!(
            !root.join("brand").exists(),
            "{name}: brand/는 들이지 않는다"
        );
        assert!(
            !root.join("characters/eunchae-mpfb.glb").exists(),
            "{name}: eunchae-mpfb.glb는 들이지 않는다"
        );
        let found = files_under(&root);
        let extra: Vec<&String> = found.difference(&expected).collect();
        let missing: Vec<&String> = expected.difference(&found).collect();
        assert!(extra.is_empty(), "{name}: 표에 없는 파일 {extra:?}");
        assert!(
            missing.is_empty(),
            "{name}: 빠진 파일 {}개 {:?}",
            missing.len(),
            &missing[..missing.len().min(5)]
        );
    }
}

/// `YYYY-MM-DD` 꼴의 날짜가 글 안에 있는가.
fn has_date(text: &str) -> bool {
    text.as_bytes().windows(10).any(|w| {
        w.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        })
    })
}

// 검증: D1
#[test]
fn source_md가_출처와_커밋_5d80963과_들인_날짜를_적는다() {
    for (name, root) in roots() {
        let text = std::fs::read_to_string(root.join("SOURCE.md"))
            .unwrap_or_else(|e| panic!("{name}/office/deskrpg/SOURCE.md가 있어야 한다: {e}"));
        assert!(text.contains("5d80963"), "{name}: 원본 커밋 5d80963");
        assert!(
            text.contains("dandacompany/deskrpg"),
            "{name}: 원본 저장소 이름"
        );
        assert!(has_date(&text), "{name}: 들인 날짜(YYYY-MM-DD)");
    }
}

// 검증: D1 D11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 서버가_외형_glb와_라이선스를_세션_없이_같은_출처_경로로_그대로_내준다() {
    let bench = Bench::start().await;
    let rows = table();
    let wanted = [
        rows.iter()
            .find(|(rel, _)| rel.starts_with("characters/office/"))
            .expect("외형 한 개"),
        rows.iter()
            .find(|(rel, _)| rel == "characters/LICENSES.md")
            .expect("라이선스"),
        rows.iter()
            .find(|(rel, _)| rel == "LICENSE.md")
            .expect("원본 LICENSE.md"),
    ];
    for (rel, hash) in wanted {
        let path = format!("/office/deskrpg/{rel}");
        let reply = get(bench.addr(), &path, None).await;
        assert_eq!(reply.status, 200, "{path}가 열린다");
        assert_eq!(&sha256_hex(&reply.bytes), hash, "{path}는 원본 그대로다");
    }
    let reply = get(bench.addr(), "/office/deskrpg/SOURCE.md", None).await;
    assert_eq!(reply.status, 200);
    assert!(reply.text().contains("5d80963"));
    bench.close().await;
}
