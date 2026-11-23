//! 빌드 결과의 파일 목록(server 11차). 쓰는 바이너리는 `screen_dist`(dist.rs)도 넣는다.

use crate::screen_dist::dist_dir;

/// `dist/<sub>` 아래 모든 파일(하위 폴더 포함): (`sub` 기준 상대 경로 `/` 구분, 내용).
pub fn dist_files(sub: &str) -> Vec<(String, Vec<u8>)> {
    let root = dist_dir().join(sub);
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("{}이 있어야 한다: {error}", dir.display()));
        for entry in entries {
            let path = entry.expect("폴더 항목").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(&root)
                .expect("dist 아래")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel, std::fs::read(&path).expect("dist 파일")));
        }
    }
    out.sort();
    out
}
