//! `infergen init` — detect stack and write a project config.

use anyhow::Context;

use infergen_core::config::Config;
use infergen_core::detect::{self, DetectionResult};

use crate::cli::{InitArgs, InitFormat};

/// Run `infergen init`.
pub fn run(args: InitArgs) -> anyhow::Result<()> {
    let dir = &args.dir;

    // 1. Refuse to overwrite an existing config unless --force.
    if let Some(existing) = Config::discover(dir) {
        if !args.force {
            anyhow::bail!(
                "config already exists at {} (use --force to overwrite)",
                existing.display()
            );
        }
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

    // 5. Summary + next step.
    println!("infergen: wrote {}", path.display());
    print_detection(&result);
    println!("next: run `infergen scan`");
    Ok(())
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
