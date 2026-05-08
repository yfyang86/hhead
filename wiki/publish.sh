#!/usr/bin/env bash
# Publish wiki/*.md to the companion GitHub Wiki repo.
#
# Requires the wiki to have been initialized at least once via the
# GitHub UI. Run from the repo root:
#
#     ./wiki/publish.sh
#
set -euo pipefail

REMOTE="${WIKI_REMOTE:-https://github.com/yfyang86/hhead.wiki.git}"
WORKDIR="$(mktemp -d)"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

echo "Cloning $REMOTE into $WORKDIR ..."
git clone --depth 1 "$REMOTE" "$WORKDIR"

# Copy every wiki/*.md except this directory's own README.md.
shopt -s nullglob
for src in "$SCRIPT_DIR"/*.md; do
    name="$(basename "$src")"
    if [[ "$name" == "README.md" ]]; then
        continue
    fi
    cp "$src" "$WORKDIR/$name"
    echo "  staged $name"
done

cd "$WORKDIR"
if git diff --quiet && git diff --cached --quiet; then
    echo "Wiki already up to date; nothing to push."
    exit 0
fi

git add -A
git commit -m "Sync wiki from main repo"
git push
echo "Wiki published."
