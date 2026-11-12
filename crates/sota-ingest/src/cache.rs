//! Local, git-ignored store of downloaded pages. An arXiv version never
//! changes, so a cached version is never fetched again.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sota_core::ArxivId;

/// Written next to each page: where it came from and what exactly was stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub url: String,
    pub fetched_at_unix: u64,
    pub sha256: String,
    pub bytes: usize,
    pub generator: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn html_path(&self, id: &ArxivId) -> PathBuf {
        self.root.join(format!("{}.html", id.file_stem()))
    }

    pub fn meta_path(&self, id: &ArxivId) -> PathBuf {
        self.root.join(format!("{}.meta.json", id.file_stem()))
    }

    pub fn contains(&self, id: &ArxivId) -> bool {
        self.html_path(id).is_file()
    }

    pub fn read_html(&self, id: &ArxivId) -> io::Result<String> {
        fs::read_to_string(self.html_path(id))
    }

    pub fn read_meta(&self, id: &ArxivId) -> io::Result<Meta> {
        let text = fs::read_to_string(self.meta_path(id))?;
        serde_json::from_str(&text).map_err(io::Error::other)
    }

    /// Stores the page and its metadata. The page is written to a temporary
    /// file and renamed, so an interrupted run never leaves half a page behind.
    pub fn put(
        &self,
        id: &ArxivId,
        url: &str,
        body: &[u8],
        generator: Option<String>,
    ) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.root)?;
        let meta = Meta {
            url: url.to_owned(),
            fetched_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            sha256: sha256_hex(body),
            bytes: body.len(),
            generator,
        };
        let path = self.html_path(id);
        let tmp = path.with_extension("html.part");
        fs::write(&tmp, body)?;
        fs::write(
            self.meta_path(id),
            serde_json::to_string_pretty(&meta).map_err(io::Error::other)?,
        )?;
        fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// Every cached paper, in id order.
    pub fn list(&self) -> io::Result<Vec<ArxivId>> {
        if !self.root.is_dir() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".html")
                && let Ok(id) = stem.replace('_', "/").parse::<ArxivId>()
            {
                ids.push(id);
            }
        }
        ids.sort_by_key(ToString::to_string);
        Ok(ids)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
