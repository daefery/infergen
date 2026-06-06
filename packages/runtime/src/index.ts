/**
 * Telemetra runtime SDK (seed).
 *
 * Providers, batching, retry, offline queue, and consent gating arrive in
 * Milestone 3. For the E0.1 scaffold this module exposes the package version
 * and the catalog-schema version it targets — mirroring
 * `telemetra-types::CATALOG_SCHEMA_VERSION` on the Rust side. Codegen (E2.x)
 * will assert the two stay in lockstep.
 */

/** Version of the on-disk catalog schema this runtime targets. */
export const CATALOG_SCHEMA_VERSION = 1 as const;

/** Semver of this runtime package. Kept in sync with package.json by release tooling. */
export const VERSION = "0.0.0" as const;
