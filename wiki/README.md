# wiki/

Source for the [GitHub Wiki](https://github.com/yfyang86/hhead/wiki). The Wiki itself lives in a separate Git repository (`hhead.wiki.git`); these files are tracked here so they can be reviewed, version-controlled with the code, and republished in one step.

## Pages

- `Home.md` — wiki landing page (becomes the wiki home).
- `Recipes.md` — practical workflows and how-tos for users.
- `FAQ.md` — frequently asked questions.
- `Format-Internals.md` — byte-level reference for every supported `--meta` format.

## Publishing to the GitHub Wiki

GitHub doesn't provide an API for wiki page creation, so publishing is done by pushing to the `.wiki.git` companion repo.

### One-time setup

1. On the [GitHub repo settings](https://github.com/yfyang86/hhead/settings) page, ensure **Wikis** is enabled under *Features*.
2. Create at least one page in the GitHub UI (any content) so the `.wiki.git` repo is initialized. Delete the placeholder afterwards.

### Push these pages to the wiki

```bash
# from the repo root
./wiki/publish.sh
```

Or manually:

```bash
git clone https://github.com/yfyang86/hhead.wiki.git /tmp/hhead.wiki
cp wiki/Home.md wiki/Recipes.md wiki/FAQ.md wiki/Format-Internals.md /tmp/hhead.wiki/
cd /tmp/hhead.wiki
git add -A
git commit -m "Sync wiki from main repo"
git push
```

GitHub renders each `*.md` file as a wiki page named after the file's basename (so `Recipes.md` → `Recipes`). Internal links use the `./Page-Name` form (no extension).

## Editing flow

Prefer editing pages here and republishing, rather than editing in the GitHub UI — that way the canonical source stays in this repo and can be reviewed in PRs alongside code changes that affect documented behavior.
