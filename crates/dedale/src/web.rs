//! The embedded browser app.
//!
//! `web/dist/` is built from `web/` with Node and committed; `build.rs`
//! compiles every file of it into the binary, so building Dedale never needs
//! Node and the app always matches the binary that serves it.

/// The app files, by path relative to `web/dist/`, sorted.
pub const FILES: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/web_files.rs"));

/// An embedded app file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub path: &'static str,
    pub bytes: &'static [u8],
    pub content_type: &'static str,
}

/// The app file at `path` (relative, as in `assets/main.js`).
#[must_use]
pub fn asset(path: &str) -> Option<Asset> {
    let index = FILES.binary_search_by_key(&path, |&(p, _)| p).ok()?;
    let (path, bytes) = FILES[index];
    Some(Asset {
        path,
        bytes,
        content_type: content_type(path),
    })
}

/// The media type served for a file, from its extension.
#[must_use]
pub fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{FILES, asset, content_type};

    fn dist() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist")
    }

    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, files);
            } else {
                files.push(path);
            }
        }
    }

    #[test]
    fn every_dist_file_is_embedded() {
        let dist = dist();
        let mut files = Vec::new();
        walk(&dist, &mut files);
        assert_eq!(files.len(), FILES.len());
        for file in files {
            let relative = file.strip_prefix(&dist).unwrap().to_str().unwrap();
            let embedded = asset(relative).unwrap_or_else(|| panic!("{relative} not embedded"));
            assert_eq!(embedded.bytes, fs::read(&file).unwrap(), "{relative}");
        }
    }

    #[test]
    fn the_app_is_there_with_known_types() {
        assert!(FILES.is_sorted_by_key(|&(path, _)| path));
        assert_eq!(
            asset("index.html").unwrap().content_type,
            "text/html; charset=utf-8"
        );
        assert!(asset("assets/main.js").is_some());
        assert_eq!(asset("missing.js"), None);
        for (path, _) in FILES {
            assert_ne!(content_type(path), "application/octet-stream", "{path}");
        }
    }
}
