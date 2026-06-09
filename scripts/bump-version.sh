#!/usr/bin/env bash
# Usage: ./scripts/bump-version.sh <version>
# Example: ./scripts/bump-version.sh 0.1.0
#
# Updates:
#   - Cargo.toml  [workspace.package] version
#   - packages/runtime/package.json  "version"
#   - packages/vscode-infergen/package.json  "version"
#
# Does NOT commit or tag — that is left to the caller (or cargo-release).
# Alternative: use `cargo release <patch|minor|major>` which does all of this
# plus CHANGELOG promotion and tag creation.

set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "Usage: $0 <version>  (e.g. 0.1.0)" >&2
  exit 1
fi

# Validate semver: major.minor.patch with optional prerelease suffix.
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$ ]]; then
  echo "Error: '$VERSION' is not a valid semver (expected major.minor.patch[-prerelease])" >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Update Cargo workspace version.
# Matches the line:  version = "x.y.z"  inside [workspace.package]
# Uses in-place sed with .bak for BSD (macOS) + GNU portability.
CARGO_TOML="$REPO_ROOT/Cargo.toml"
sed -i.bak "s/^version = \"[0-9][^\"]*\"$/version = \"$VERSION\"/" "$CARGO_TOML"
rm -f "$CARGO_TOML.bak"
echo "Updated Cargo.toml → version = \"$VERSION\""

# Update npm packages.
for pkg in packages/runtime packages/vscode-infergen; do
  PKG_JSON="$REPO_ROOT/$pkg/package.json"
  if [[ -f "$PKG_JSON" ]]; then
    sed -i.bak "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$PKG_JSON"
    rm -f "$PKG_JSON.bak"
    echo "Updated $pkg/package.json → \"version\": \"$VERSION\""
  else
    echo "Warning: $PKG_JSON not found — skipping" >&2
  fi
done

echo ""
echo "Version bumped to $VERSION. Next steps:"
echo "  cargo check"
echo "  git add Cargo.toml packages/*/package.json"
echo "  git commit -m 'chore: bump version to $VERSION'"
echo "  git tag v$VERSION"
echo "  git push && git push --tags"
