//! Embeds the browser app: lists every file of `web/dist/` (built and
//! committed, so building Dedale never needs Node) for `include_bytes!`.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::{env, io};

fn main() -> io::Result<()> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let dist = manifest.join("../../web/dist").canonicalize()?;
    println!("cargo::rerun-if-changed={}", dist.display());

    let mut paths = Vec::new();
    collect(&dist, &mut paths)?;
    let mut files: Vec<(String, &Path)> = paths
        .iter()
        .map(|path| {
            let relative = path.strip_prefix(&dist).expect("under dist");
            let Some(relative) = relative.to_str().filter(|r| !r.contains('\\')) else {
                panic!("unsupported file name in web/dist: {}", relative.display());
            };
            (relative.to_owned(), path.as_path())
        })
        .collect();
    // By string, as `web::assets::asset` searches (paths order by component:
    // `live/x.js` before `live.js`).
    files.sort();
    let mut table = String::from("&[\n");
    for (relative, path) in &files {
        let absolute = path.to_str().expect("UTF-8 path");
        writeln!(table, "    ({relative:?}, include_bytes!({absolute:?})),").expect("in memory");
    }
    table.push_str("]\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));
    fs::write(out.join("web_files.rs"), table)
}

/// Every file under `dir`, recursively.
fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}
