//! Reads the config directory from disk and hands the text to `sota-core`.
//!
//! Layout: `ontology.toml`, `normalization.toml`, `registry/*.toml` and
//! `methods/*.toml` (one per topic).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sota_core::{MethodRegistry, Normalizer, Ontology, Registry};

pub struct Config {
    pub ontology: Ontology,
    pub registry: Registry,
    pub methods: MethodRegistry,
}

pub fn load(dir: &Path) -> Result<Config> {
    if !dir.is_dir() {
        bail!("config directory {} does not exist", dir.display());
    }

    let path = dir.join("ontology.toml");
    let ontology = Ontology::from_toml_str(&name(&path), &read(&path)?)?;

    let path = dir.join("normalization.toml");
    let normalizer = Normalizer::from_toml_str(&name(&path), &read(&path)?)?;

    let texts = read_all(&registry_files(&dir.join("registry"))?)?;
    let registry = Registry::build(normalizer.clone(), &sources(&texts))?;

    let texts = read_all(&registry_files(&dir.join("methods"))?)?;
    let methods = MethodRegistry::build(normalizer, &sources(&texts))?;

    Ok(Config {
        ontology,
        registry,
        methods,
    })
}

fn read_all(paths: &[PathBuf]) -> Result<Vec<(String, String)>> {
    paths.iter().map(|p| Ok((name(p), read(p)?))).collect()
}

fn sources(texts: &[(String, String)]) -> Vec<(&str, &str)> {
    texts
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect()
}

fn registry_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "toml") {
            files.push(path);
        }
    }
    if files.is_empty() {
        bail!("no registry files (*.toml) in {}", dir.display());
    }
    files.sort();
    Ok(files)
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn name(path: &Path) -> String {
    path.display().to_string()
}
