# Infergen Express Example

Minimal example showing what `infergen init` produces for an Express + TypeScript project.

## What's here

```
infergen.config.json          # written by infergen init
.infergen/catalog.yaml        # example events scaffolded by infergen init
package.json                  # declares express + typescript (used for stack detection)
```

## Try it

```bash
# 1. Install infergen (from source)
cargo install infergen

# 2. Run scan on this directory (or your real project)
cd examples/express-app
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
import express from "express";

const app = express();

app.use((req, res, next) => {
  track.apiRequestReceived({ method: req.method, path: req.path });
  next();
});

app.post("/login", (req, res) => {
  track.userLoginAttempted({ success: true });
  res.json({ ok: true });
});
```

## Full quickstart

See [docs/quickstart.md](../../docs/quickstart.md) for a step-by-step guide covering
all supported stacks.
