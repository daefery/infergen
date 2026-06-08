//! `infergen init` — detect stack and write a project config.

use std::fs;

use anyhow::Context;

use infergen_core::config::Config;
use infergen_core::detect::{self, DetectionResult, Framework};
use infergen_core::templates;

use crate::cli::{InitArgs, InitFormat};

/// Run `infergen init`.
pub fn run(args: InitArgs) -> anyhow::Result<()> {
    let dir = &args.dir;

    // 1. Refuse to overwrite an existing config unless --force.
    if let Some(existing) = Config::discover(dir)
        && !args.force
    {
        anyhow::bail!(
            "config already exists at {} (use --force to overwrite)",
            existing.display()
        );
    }

    // 2. Detect the stack (tolerant; only errors if `dir` is missing).
    let result =
        detect::detect(dir).with_context(|| format!("failed to inspect {}", dir.display()))?;

    // 3. Build a config from defaults overlaid with detection.
    let config = Config {
        languages: result.languages.clone(),
        frameworks: result.frameworks.clone(),
        ..Config::default()
    };

    // 4. Choose the filename and write.
    let filename = match args.format {
        InitFormat::Json => "infergen.config.json",
        InitFormat::Toml => "infergen.config.toml",
    };
    let path = dir.join(filename);
    config
        .save(&path)
        .with_context(|| format!("writing {}", path.display()))?;

    // 5. Summary.
    println!("infergen: wrote {}", path.display());
    print_detection(&result);

    // 6. Scaffold example catalog unless suppressed.
    if !args.no_example {
        scaffold_example(dir, &result, &config)?;
    }

    // 7. Stack-specific quickstart steps.
    print_quickstart(&result.frameworks);

    Ok(())
}

/// Write an example `.infergen/catalog.yaml` seeded with stack-appropriate
/// proposed events. Skips silently if the file already exists.
fn scaffold_example(
    dir: &std::path::Path,
    result: &DetectionResult,
    config: &Config,
) -> anyhow::Result<()> {
    let catalog_path = dir.join(&config.catalog);

    if catalog_path.exists() {
        println!(
            "infergen: example catalog already exists at {}, skipping",
            catalog_path.display()
        );
        return Ok(());
    }

    if let Some(parent) = catalog_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }

    let template = templates::template_for_frameworks(&result.frameworks);
    fs::write(&catalog_path, template.example_catalog_yaml)
        .with_context(|| format!("writing example catalog to {}", catalog_path.display()))?;

    println!("infergen: wrote example catalog to {}", catalog_path.display());
    Ok(())
}

/// Print ordered quickstart steps for the detected stack.
fn print_quickstart(frameworks: &[Framework]) {
    let template = templates::template_for_frameworks(frameworks);
    println!("\nQuickstart ({})\n", template.stack_name);
    for (i, step) in template.quickstart_steps.iter().enumerate() {
        println!("  {}. {}", i + 1, step);
    }
    println!();
}

/// Print a human summary of what detection found.
fn print_detection(result: &DetectionResult) {
    if result.languages.is_empty() && result.frameworks.is_empty() {
        println!("detected: no known languages/frameworks");
        return;
    }
    if !result.languages.is_empty() {
        let langs: Vec<String> = result.languages.iter().map(|l| format!("{l:?}")).collect();
        println!("languages: {}", langs.join(", "));
    }
    if !result.frameworks.is_empty() {
        let fws: Vec<String> = result.frameworks.iter().map(|f| format!("{f:?}")).collect();
        println!("frameworks: {}", fws.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use infergen_core::detect::Framework;
    use std::fs;
    use tempfile::TempDir;

    fn nextjs_dir() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let pkg = serde_json::json!({
            "name": "test-app",
            "dependencies": { "next": "^14.0.0", "react": "^18.0.0" },
            "devDependencies": { "typescript": "^5.0.0" }
        });
        fs::write(dir.path().join("package.json"), pkg.to_string()).unwrap();
        dir
    }

    #[test]
    fn scaffold_writes_catalog_yaml() {
        let dir = nextjs_dir();
        let result = detect::detect(dir.path()).unwrap();
        let config = Config::default();
        scaffold_example(dir.path(), &result, &config).unwrap();
        let catalog = dir.path().join(".infergen/catalog.yaml");
        assert!(catalog.exists(), "catalog.yaml should be created");
        let content = fs::read_to_string(&catalog).unwrap();
        assert!(!content.is_empty(), "catalog.yaml should not be empty");
        assert!(content.contains("schemaVersion"), "should contain schemaVersion");
    }

    #[test]
    fn scaffold_skips_if_catalog_exists() {
        let dir = nextjs_dir();
        let result = detect::detect(dir.path()).unwrap();
        let config = Config::default();
        let catalog_path = dir.path().join(".infergen/catalog.yaml");
        fs::create_dir_all(catalog_path.parent().unwrap()).unwrap();
        let original = "original content";
        fs::write(&catalog_path, original).unwrap();
        scaffold_example(dir.path(), &result, &config).unwrap();
        let content = fs::read_to_string(&catalog_path).unwrap();
        assert_eq!(content, original, "existing catalog should not be overwritten");
    }

    #[test]
    fn scaffold_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let result = detect::detect(dir.path()).unwrap();
        let config = Config::default();
        // .infergen/ does not exist yet
        assert!(!dir.path().join(".infergen").exists());
        scaffold_example(dir.path(), &result, &config).unwrap();
        assert!(dir.path().join(".infergen/catalog.yaml").exists());
    }

    #[test]
    fn run_writes_config_and_catalog() {
        let dir = nextjs_dir();
        let args = InitArgs {
            dir: dir.path().to_path_buf(),
            format: crate::cli::InitFormat::Json,
            force: false,
            no_example: false,
        };
        run(args).unwrap();
        assert!(dir.path().join("infergen.config.json").exists());
        assert!(dir.path().join(".infergen/catalog.yaml").exists());
    }

    #[test]
    fn no_example_flag_skips_scaffold() {
        let dir = nextjs_dir();
        let args = InitArgs {
            dir: dir.path().to_path_buf(),
            format: crate::cli::InitFormat::Json,
            force: false,
            no_example: true,
        };
        run(args).unwrap();
        assert!(dir.path().join("infergen.config.json").exists());
        assert!(!dir.path().join(".infergen/catalog.yaml").exists());
    }

    #[test]
    fn print_quickstart_generic_for_empty_frameworks() {
        // Must not panic.
        print_quickstart(&[]);
    }

    #[test]
    fn no_example_flag_default_false() {
        // InitArgs constructed with no_example: false is the default.
        let dir = tempfile::tempdir().unwrap();
        let args = InitArgs {
            dir: dir.path().to_path_buf(),
            format: crate::cli::InitFormat::Json,
            force: false,
            no_example: false,
        };
        assert!(!args.no_example);
    }

    #[test]
    fn scaffold_nextjs_catalog_has_page_viewed() {
        let result = DetectionResult {
            languages: vec![],
            frameworks: vec![Framework::NextJs],
        };
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        scaffold_example(dir.path(), &result, &config).unwrap();
        let content = fs::read_to_string(dir.path().join(".infergen/catalog.yaml")).unwrap();
        assert!(content.contains("page_viewed"));
    }
}
