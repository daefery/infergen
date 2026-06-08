# Infergen Next.js Example

Minimal example showing what `infergen init` produces for a Next.js + TypeScript project.

## What's here

```
infergen.config.json          # written by infergen init
.infergen/catalog.yaml        # example events scaffolded by infergen init
package.json                  # declares next + typescript (used for stack detection)
```

## Try it

```bash
# 1. Install infergen (from source)
cargo install infergen

# 2. Run scan on this directory (or your real project)
cd examples/nextjs-app
infergen scan

# 3. Review proposed events
infergen review list
infergen review approve --all

# 4. Generate a typed SDK
infergen generate

# 5. Use in your code
```

```ts
import { track } from "./infergen.generated";

// Track a page view
track.pageViewed({ page_path: "/dashboard" });

// Track signup
track.userSignupCompleted({ method: "google" });
```

## Full quickstart

See [docs/quickstart.md](../../docs/quickstart.md) for a step-by-step guide covering
all supported stacks.
