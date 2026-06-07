//! `infergen schema` — SQL schema generation from approved catalog (E3.2b).

use anyhow::Context;
use infergen_core::{Catalog, SqlDialect, generate_sql_schema, load_catalog};

use crate::cli::{SchemaArgs, SchemaDialect};

/// Run `infergen schema`.
pub fn run(args: SchemaArgs) -> anyhow::Result<()> {
    let catalog = if args.catalog.exists() {
        load_catalog(&args.catalog)
            .with_context(|| format!("reading catalog {}", args.catalog.display()))?
    } else {
        Catalog::default()
    };

    let dialect = match args.dialect {
        SchemaDialect::Postgres => SqlDialect::Postgres,
        SchemaDialect::Mysql => SqlDialect::Mysql,
        SchemaDialect::Sqlite => SqlDialect::Sqlite,
    };

    let sql = generate_sql_schema(&catalog, dialect);

    match args.output {
        Some(ref path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating output directory {}", parent.display()))?;
            }
            std::fs::write(path, &sql)
                .with_context(|| format!("writing {}", path.display()))?;
            println!("infergen: wrote {}", path.display());
        }
        None => print!("{sql}"),
    }

    Ok(())
}
