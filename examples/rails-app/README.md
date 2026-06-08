# Infergen Rails Example

Minimal example showing what `infergen init` produces for a Ruby on Rails project.

## What's here

```
infergen.config.json          # written by infergen init
.infergen/catalog.yaml        # example events scaffolded by infergen init
Gemfile                       # declares rails + devise (used for stack detection)
```

## Try it

```bash
# 1. Install infergen (from source)
cargo install infergen

# 2. Run scan on this directory (or your real Rails project)
cd examples/rails-app
infergen scan

# 3. Review proposed events
infergen review list
infergen review approve --all

# 4. Generate a typed Ruby SDK
infergen generate

# 5. Use in your code
```

```ruby
require_relative "infergen/generated"

class ApplicationController < ActionController::Base
  before_action :track_action

  private

  def track_action
    Infergen::Track.controller_action_called(
      controller: controller_name,
      action: action_name
    )
  end
end

# After Devise sign-in:
def after_sign_in_path_for(resource)
  Infergen::Track.user_signed_in(user_id: current_user.id.to_s)
  root_path
end
```

## Full quickstart

See [docs/quickstart.md](../../docs/quickstart.md) for a step-by-step guide covering
all supported stacks.
