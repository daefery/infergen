# Telemetra

A framework-agnostic library that scans any codebase offline, infers a typed analytics event catalog, and generates a type-safe, multi-provider SDK.

**🚧 Pre-alpha — scaffold only (E0.1). Commands land in E0.2.**

![CI](https://github.com/telemetra/telemetra/actions/workflows/ci.yml/badge.svg)

---

## Repository layout

```
telemetra/
├── Cargo.toml                      # Rust workspace manifest
├── rust-toolchain.toml             # pinned toolchain (1.85.0) + components
├── rustfmt.toml                    # Rust formatting config (edition 2024)
├── deny.toml                       # cargo-deny: license + advisory gate
├── .gitignore
├── .editorconfig
├── LICENSE                         # Apache-2.0
├── README.md                       # this file
├── CONTRIBUTING.md                 # dev setup, conventions, release process
├── Justfile                        # cross-language task runner
├── package.json                    # root JS workspace orchestration (private)
├── pnpm-workspace.yaml
├── .github/workflows/ci.yml        # Rust + JS CI
├── .github/workflows/release.yml   # cargo-dist binary release (auto-generated)
├── crates/
│   ├── telemetra-types/            # shared, dependency-free domain types
│   ├── telemetra-core/             # scan-engine library (parsers/adapters land in E0.3+)
│   └── telemetra-cli/              # `telemetra` binary (commands land in E0.2)
└── packages/
    └── runtime/                    # @telemetra/runtime — TS runtime SDK seed
```

## Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | ≥1.85 | `rustup` auto-installs from `rust-toolchain.toml` — [rustup.rs](https://rustup.rs) |
| Node | ≥20 | [nodejs.org](https://nodejs.org) or `nvm install 20` |
| pnpm | 9 | `corepack enable && corepack prepare pnpm@9.15.0 --activate` |
| just | latest | `cargo install just` or `brew install just` |
| cargo-deny | latest | `cargo install cargo-deny` (needed for `just deny`) |

## Quickstart

```bash
just install      # pnpm install
just build        # cargo + pnpm build
just test         # all tests
just ci           # full local CI parity
cargo run -p telemetra-cli -- --version
```

## Architecture

See [`PRD.md`](./PRD.md) §8 and [`ROADMAP.md`](./ROADMAP.md) for the full architecture.

- `telemetra-types` — leaf crate; shared domain types (catalog schema version, future event structs)
- `telemetra-core` — scan engine (parsers, adapters, namer, codegen — arriving E0.3–E2.x)
- `telemetra-cli` — the `telemetra` binary (subcommands arriving E0.2)
- `@telemetra/runtime` — TypeScript runtime SDK (providers, batching arriving M3)

## License

Apache-2.0 — see [LICENSE](./LICENSE).
