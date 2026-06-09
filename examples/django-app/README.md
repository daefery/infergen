# Infergen Django Example

Minimal example showing what `infergen init` produces for a Python/Django project.

## What's here

```
infergen.config.json          # written by infergen init
.infergen/catalog.yaml        # example events scaffolded by infergen init
requirements.txt              # declares django (used for stack detection)
```

## Try it

```bash
# 1. Install infergen (from source)
cargo install infergen

# 2. Run scan on this directory (or your real Django project)
cd examples/django-app
infergen scan

# 3. Review proposed events
infergen review list
infergen review approve --all

# 4. Generate a typed Python SDK
infergen generate

# 5. Use in your code
```

```python
from infergen_generated import track

# In a view:
def my_view(request):
    track.view_requested(view_name="my_view", method=request.method)
    # ...

# After login:
def login_view(request):
    success = authenticate(request)
    track.user_login_attempted(success=success)
```

## Full quickstart

See [docs/quickstart.md](../../docs/quickstart.md) for a step-by-step guide covering
all supported stacks.
