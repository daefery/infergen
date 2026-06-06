# Contributing to Infergen

## Dev environment

1. **Rust** — install [rustup](https://rustup.rs). After checkout, `rustup` auto-installs the pinned toolchain from `rust-toolchain.toml` (1.85.0 + rustfmt + clippy).
2. **Node ≥ 20** — via [nvm](https://github.com/nvm-sh/nvm) or [nodejs.org](https://nodejs.org).
3. **pnpm 9** — enable via Corepack: `corepack enable && corepack prepare pnpm@9.15.0 --activate`.
4. **just** — `cargo install just` or `brew install just`.
5. **cargo-deny** — `cargo install cargo-deny`.
6. Run `just install` to install JS dependencies.

## The check loop

Before every push, run:

```bash
just ci
```

This runs, in order:

1. `cargo fmt --all -- --check` — Rust formatting
2. `cargo clippy --workspace --all-targets -- -D warnings` — Rust lints (warnings-as-errors)
3. `pnpm -r typecheck` — TypeScript type check
4. `cargo deny check` — license + advisory gate
5. `cargo test --workspace` — all Rust unit + integration tests
6. `pnpm -r test` — all JS/TS tests
7. `cargo build --workspace` + `pnpm -r build` — full build

All steps must be green before opening a PR.

## Code conventions

- **Rust**: formatted by `rustfmt` (edition 2024, max width 100); `unsafe_code = "forbid"` — no unsafe blocks; `missing_docs` warns — document all public items.
- **TypeScript**: `strict: true` + `noUncheckedIndexedAccess`; `verbatimModuleSyntax`.
- **Commits**: [Conventional Commits](https://www.conventionalcommits.org/) — `feat:`, `fix:`, `chore:`, `docs:`, etc.

## Adding a dependency

- Rust: add to `[workspace.dependencies]` in `Cargo.toml`, then reference via `{ workspace = true }` in the crate. Run `cargo deny check licenses` — it must pass.
- Only Apache-2.0-compatible licenses are allowed (no GPL/AGPL). If a new permissive license appears (e.g. `MPL-2.0`), add it to the `allow` list in `deny.toml` with a comment explaining why it's compatible.

## Release (interim — E0.1)

**Binaries**: push a `v*` tag → the `release.yml` workflow triggers → `cargo-dist` builds and publishes a GitHub Release with per-platform archives and installer scripts.

**npm (`@infergen/runtime`)**: until E8.3 automates it, publish manually:

```bash
pnpm -r build
pnpm -C packages/runtime publish
```

Full release automation (npm + binary signing + changelog) is tracked in **E8.3** of the roadmap.

## License of contributions

All contributions are licensed under Apache-2.0.

> **Note:** A CLA/DCO policy is being decided in **E9.1** (see [ROADMAP.md](./ROADMAP.md)). Until that epic ships, contributions are accepted on the implicit understanding that they are Apache-2.0 licensed.
