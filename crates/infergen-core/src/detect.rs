//! Language & framework auto-detection for `infergen init`.
//!
//! Detection is heuristic and data-driven: it inspects marker files
//! (`package.json`, `tsconfig.json`, `go.mod`, `pyproject.toml`, `Gemfile`, …)
//! and dependency manifests. It never parses source ASTs (that is E0.3) and
//! never fails on a malformed manifest — unreadable inputs are simply skipped.
//! Next.js + TypeScript is first-class (PRD §13); other stacks are best-effort.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;

/// A source language Infergen can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// TypeScript.
    TypeScript,
    /// JavaScript.
    JavaScript,
    /// Python.
    Python,
    /// Go.
    Go,
    /// Ruby.
    Ruby,
}

/// A framework/runtime Infergen has (or will have) an adapter for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    /// Next.js (implies React).
    NextJs,
    /// React.
    React,
    /// Express.
    Express,
    /// NestJS.
    NestJs,
    /// Vue / Nuxt.
    Vue,
    /// SvelteKit / Svelte.
    SvelteKit,
    /// Django.
    Django,
    /// FastAPI.
    FastApi,
    /// Flask.
    Flask,
    /// Gin.
    Gin,
    /// Echo.
    Echo,
    /// Ruby on Rails.
    Rails,
}

/// Result of scanning a project root for languages + frameworks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DetectionResult {
    /// Detected languages, de-duplicated, in detection order.
    pub languages: Vec<Language>,
    /// Detected frameworks, de-duplicated, in detection order.
    pub frameworks: Vec<Framework>,
}

impl DetectionResult {
    fn add_language(&mut self, lang: Language) {
        if !self.languages.contains(&lang) {
            self.languages.push(lang);
        }
    }

    fn add_framework(&mut self, fw: Framework) {
        if !self.frameworks.contains(&fw) {
            self.frameworks.push(fw);
        }
    }
}

/// Detect languages + frameworks under `root`.
///
/// # Errors
/// Returns [`Error::Io`] only if `root` does not exist or is not a directory.
/// Individual unreadable or malformed manifests are skipped, never fatal.
pub fn detect(root: &Path) -> Result<DetectionResult, Error> {
    if !root.is_dir() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("not a directory: {}", root.display()),
        )));
    }

    let mut result = DetectionResult::default();
    detect_js(root, &mut result);
    detect_python(root, &mut result);
    detect_go(root, &mut result);
    detect_ruby(root, &mut result);
    Ok(result)
}

/// Read a file to a string, returning `None` (skip) on any error.
fn read_opt(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn detect_js(root: &Path, result: &mut DetectionResult) {
    let pkg_path = root.join("package.json");
    if !pkg_path.is_file() {
        return;
    }

    // Gather dependency names (tolerant of malformed JSON).
    let mut deps: BTreeSet<String> = BTreeSet::new();
    if let Some(text) = read_opt(&pkg_path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            for key in ["dependencies", "devDependencies"] {
                if let Some(map) = value.get(key).and_then(|v| v.as_object()) {
                    deps.extend(map.keys().cloned());
                }
            }
        }
    }

    // Language: TypeScript if the `typescript` dep or a tsconfig.json exists.
    if deps.contains("typescript") || root.join("tsconfig.json").is_file() {
        result.add_language(Language::TypeScript);
    } else {
        result.add_language(Language::JavaScript);
    }

    // Frameworks.
    if deps.contains("next") {
        result.add_framework(Framework::NextJs);
        result.add_framework(Framework::React);
    } else if deps.contains("react") {
        result.add_framework(Framework::React);
    }
    if deps.contains("express") {
        result.add_framework(Framework::Express);
    }
    if deps.contains("@nestjs/core") {
        result.add_framework(Framework::NestJs);
    }
    if deps.contains("vue") || deps.contains("nuxt") {
        result.add_framework(Framework::Vue);
    }
    if deps.contains("@sveltejs/kit") || deps.contains("svelte") {
        result.add_framework(Framework::SvelteKit);
    }
}

fn detect_python(root: &Path, result: &mut DetectionResult) {
    let markers = ["pyproject.toml", "requirements.txt", "setup.py"];
    let present: Vec<&str> = markers
        .iter()
        .copied()
        .filter(|m| root.join(m).is_file())
        .collect();
    if present.is_empty() {
        return;
    }
    result.add_language(Language::Python);

    let mut text = String::new();
    for m in present {
        if let Some(content) = read_opt(&root.join(m)) {
            text.push_str(&content.to_lowercase());
            text.push('\n');
        }
    }
    if text.contains("django") {
        result.add_framework(Framework::Django);
    }
    if text.contains("fastapi") {
        result.add_framework(Framework::FastApi);
    }
    if text.contains("flask") {
        result.add_framework(Framework::Flask);
    }
}

fn detect_go(root: &Path, result: &mut DetectionResult) {
    let go_mod = root.join("go.mod");
    if !go_mod.is_file() {
        return;
    }
    result.add_language(Language::Go);
    if let Some(text) = read_opt(&go_mod) {
        let text = text.to_lowercase();
        if text.contains("gin-gonic/gin") {
            result.add_framework(Framework::Gin);
        }
        if text.contains("labstack/echo") {
            result.add_framework(Framework::Echo);
        }
    }
}

fn detect_ruby(root: &Path, result: &mut DetectionResult) {
    let gemfile = root.join("Gemfile");
    if !gemfile.is_file() {
        return;
    }
    result.add_language(Language::Ruby);
    if let Some(text) = read_opt(&gemfile) {
        if text.to_lowercase().contains("rails") {
            result.add_framework(Framework::Rails);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn detects_nextjs_typescript() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"dependencies":{"next":"14"},"devDependencies":{"typescript":"5"}}"#,
        );
        let r = detect(dir.path()).unwrap();
        assert!(r.languages.contains(&Language::TypeScript));
        assert!(r.frameworks.contains(&Framework::NextJs));
        assert!(r.frameworks.contains(&Framework::React));
    }

    #[test]
    fn detects_javascript_without_typescript() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"dependencies":{"react":"18"}}"#,
        );
        let r = detect(dir.path()).unwrap();
        assert!(r.languages.contains(&Language::JavaScript));
        assert!(!r.languages.contains(&Language::TypeScript));
        assert!(r.frameworks.contains(&Framework::React));
    }

    #[test]
    fn tsconfig_implies_typescript() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"dependencies":{"react":"18"}}"#,
        );
        write(dir.path(), "tsconfig.json", "{}");
        let r = detect(dir.path()).unwrap();
        assert!(r.languages.contains(&Language::TypeScript));
    }

    #[test]
    fn detects_python_fastapi() {
        let dir = tempdir().unwrap();
        write(dir.path(), "requirements.txt", "fastapi==0.110\nuvicorn\n");
        let r = detect(dir.path()).unwrap();
        assert!(r.languages.contains(&Language::Python));
        assert!(r.frameworks.contains(&Framework::FastApi));
    }

    #[test]
    fn detects_go() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "go.mod",
            "module x\n\nrequire github.com/gin-gonic/gin v1.9.0\n",
        );
        let r = detect(dir.path()).unwrap();
        assert!(r.languages.contains(&Language::Go));
        assert!(r.frameworks.contains(&Framework::Gin));
    }

    #[test]
    fn detects_ruby_rails() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "Gemfile",
            "source 'https://rubygems.org'\ngem \"rails\"\n",
        );
        let r = detect(dir.path()).unwrap();
        assert!(r.languages.contains(&Language::Ruby));
        assert!(r.frameworks.contains(&Framework::Rails));
    }

    #[test]
    fn empty_dir_detects_nothing() {
        let dir = tempdir().unwrap();
        let r = detect(dir.path()).unwrap();
        assert_eq!(r, DetectionResult::default());
    }

    #[test]
    fn malformed_package_json_is_tolerant() {
        let dir = tempdir().unwrap();
        write(dir.path(), "package.json", "{ broken");
        write(dir.path(), "tsconfig.json", "{}");
        let r = detect(dir.path()).unwrap();
        assert!(r.languages.contains(&Language::TypeScript));
        assert!(r.frameworks.is_empty());
    }

    #[test]
    fn missing_root_errors() {
        let err = detect(Path::new("/no/such/dir/infergen-test")).unwrap_err();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn enum_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&Language::TypeScript).unwrap(),
            "\"type-script\""
        );
        assert_eq!(
            serde_json::to_string(&Framework::NextJs).unwrap(),
            "\"next-js\""
        );
    }
}
