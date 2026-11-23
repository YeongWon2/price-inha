//! 화면 빌드 결과(`web/dist`, Vite)를 실행 파일에 넣는 파일 표를 만든다(server 11차 3번).
//!
//! `web/dist` 아래 모든 파일을 (상대 경로, `include_bytes!`) 표로 적어 `$OUT_DIR/web_dist.rs`에 쓴다.
//! 바깥 의존을 더하지 않는다. dist가 바뀌면 다시 돈다(`rerun-if-changed`가 폴더면 그 안 전체를 본다).
//! dist는 커밋한다 — 없으면 `npm --prefix crates/server/web run build`로 만든다.

use std::fmt::Write as _;
use std::io::{Error, Write as _};
use std::path::{Path, PathBuf};

const HOW_TO_BUILD: &str = "npm --prefix crates/server/web run build로 화면을 빌드한다";

/// `dir` 아래 모든 파일: (`root` 기준 `/` 구분 상대 경로, 절대 경로).
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|error| {
        Error::other(format!(
            "{}를 읽지 못했다({error}). {HOW_TO_BUILD}",
            dir.display()
        ))
    })?;
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(Error::other)?
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        out.push((rel, path));
    }
    Ok(())
}

fn var(name: &str) -> std::io::Result<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| Error::other(format!("{name}가 없다")))
}

fn main() -> std::io::Result<()> {
    let dist = var("CARGO_MANIFEST_DIR")?.join("web/dist");
    let mut cargo = std::io::stdout();
    writeln!(cargo, "cargo:rerun-if-changed=web/dist")?;
    writeln!(cargo, "cargo:rerun-if-changed=build.rs")?;

    let mut files = Vec::new();
    collect(&dist, &dist, &mut files)?;
    files.sort();
    if !files.iter().any(|(rel, _)| rel == "index.html") {
        return Err(Error::other(format!(
            "web/dist/index.html이 없다. {HOW_TO_BUILD}"
        )));
    }

    let mut table = String::from(
        "/// 화면 빌드 결과(`web/dist`)의 (상대 경로, 내용). build.rs가 만든다.\n\
         pub(crate) static DIST_FILES: &[(&str, &[u8])] = &[\n",
    );
    for (rel, path) in &files {
        let absolute = path.to_string_lossy();
        writeln!(table, "    ({rel:?}, include_bytes!({absolute:?})),").map_err(Error::other)?;
    }
    table.push_str("];\n");
    std::fs::write(var("OUT_DIR")?.join("web_dist.rs"), table)
}
