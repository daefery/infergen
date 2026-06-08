import * as fs from 'fs';
import * as path from 'path';

export interface InfergenExtConfig {
  /** Absolute path to the catalog YAML file. */
  catalogPath: string;
}

/**
 * Resolve infergen configuration for the given workspace root.
 *
 * Priority:
 * 1. Read `infergen.config.json` → extract `catalog` field
 * 2. Default: `${workspaceRoot}/.infergen/catalog.yaml`
 *
 * TOML config is not parsed from Node; falls back to default when present.
 * The returned `catalogPath` is always absolute.
 */
export function resolveConfig(workspaceRoot: string): InfergenExtConfig {
  const defaultPath = path.join(workspaceRoot, '.infergen', 'catalog.yaml');

  const jsonPath = path.join(workspaceRoot, 'infergen.config.json');
  if (fs.existsSync(jsonPath)) {
    try {
      const raw = JSON.parse(fs.readFileSync(jsonPath, 'utf-8')) as Record<string, unknown>;
      if (typeof raw['catalog'] === 'string' && raw['catalog'].length > 0) {
        return { catalogPath: path.resolve(workspaceRoot, raw['catalog']) };
      }
    } catch {
      // fall through to default
    }
  }

  return { catalogPath: defaultPath };
}
