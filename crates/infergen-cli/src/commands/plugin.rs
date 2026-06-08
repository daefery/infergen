//! `infergen plugin` — scaffold and describe plugin extension points (E7.3).

use std::fs;

use anyhow::{Context, bail};
use infergen_core::plugin::{adapter_scaffold, parser_scaffold, provider_scaffold};

use crate::cli::{PluginAction, PluginArgs, PluginKind};

/// Run `infergen plugin <action>`.
pub fn run(args: PluginArgs) -> anyhow::Result<()> {
    match args.action {
        PluginAction::Scaffold {
            kind,
            name,
            framework,
            output,
        } => scaffold(kind, &name, framework.as_deref(), output.as_deref()),
        PluginAction::ListTypes => list_types(),
    }
}

fn scaffold(
    kind: PluginKind,
    name: &str,
    framework: Option<&str>,
    output: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    validate_name(name)?;

    let source = match kind {
        PluginKind::Provider => provider_scaffold(name),
        PluginKind::Adapter => {
            let fw = framework.ok_or_else(|| {
                anyhow::anyhow!(
                    "`infergen plugin scaffold adapter` requires --framework <name>\n\
                     Example: infergen plugin scaffold adapter my-adapter --framework htmx"
                )
            })?;
            adapter_scaffold(name, fw)
        }
        PluginKind::Parser => parser_scaffold(name, name),
    };

    match output {
        Some(path) => {
            fs::write(path, &source)
                .with_context(|| format!("could not write scaffold to {}", path.display()))?;
            println!("Scaffold written to {}", path.display());
        }
        None => print!("{source}"),
    }
    Ok(())
}

fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        bail!("Plugin name must not be empty.");
    }
    for ch in name.chars() {
        if !matches!(ch, 'a'..='z' | '0'..='9' | '-' | '_') {
            bail!(
                "Plugin name {name:?} contains invalid character {ch:?}. \
                 Use only lowercase letters, digits, hyphens, and underscores."
            );
        }
    }
    Ok(())
}

fn list_types() -> anyhow::Result<()> {
    println!("Available plugin types:");
    println!();
    println!("  provider  Add an analytics destination.");
    println!("            Trait:   infergen_core::ProviderPlugin");
    println!("            Methods: id(), track()  [+ optional flush(), shutdown()]");
    println!("            Scaffold: infergen plugin scaffold provider <name>");
    println!();
    println!("  adapter   Detect trackable moments in a new framework.");
    println!("            Trait:   infergen_core::Adapter");
    println!("            Methods: framework(), analyze()");
    println!("            Scaffold: infergen plugin scaffold adapter <name> --framework <fw>");
    println!();
    println!("  parser    Support a new source language.");
    println!("            Trait:   infergen_core::LanguageParser");
    println!("            Methods: parse()");
    println!("            Scaffold: infergen plugin scaffold parser <name>");
    println!();
    println!("See docs/plugin-sdk.md for the full Plugin SDK guide.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_accepts_valid_names() {
        assert!(validate_name("my-provider").is_ok());
        assert!(validate_name("debug_log").is_ok());
        assert!(validate_name("posthog").is_ok());
        assert!(validate_name("provider123").is_ok());
    }

    #[test]
    fn validate_name_rejects_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn validate_name_rejects_uppercase() {
        assert!(validate_name("MyProvider").is_err());
    }

    #[test]
    fn validate_name_rejects_space() {
        assert!(validate_name("my provider").is_err());
    }

    #[test]
    fn validate_name_rejects_dot() {
        assert!(validate_name("my.provider").is_err());
    }
}
