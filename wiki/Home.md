# hhead Wiki

Welcome to the `hhead` wiki. This is the long-form companion to the project's [README](https://github.com/yfyang86/hhead/blob/main/Readme.md) and [DEVELOPMENT.md](https://github.com/yfyang86/hhead/blob/main/DEVELOPMENT.md).

## For users

- **[Recipes](./Recipes)** — concrete how-tos: triaging a corrupt PNG, peeking inside a ZIP, spotting UTF-8 BOMs, reading minimaps, and so on.
- **[FAQ](./FAQ)** — frequently asked questions about flags, output, performance, and limitations.
- **[Format Internals](./Format-Internals)** — what `hhead --meta` actually inspects for each supported format, with byte offsets.

## For developers

- **[DEVELOPMENT.md](https://github.com/yfyang86/hhead/blob/main/DEVELOPMENT.md)** in the main repo covers building, the module tree, the `write_hex<W: Write>` testing pattern, coding conventions, and how to add a new format parser.

## Quick links

- Source: <https://github.com/yfyang86/hhead>
- Issues: <https://github.com/yfyang86/hhead/issues>
- License: MIT

> Wiki pages here are kept short and stable. Anything that changes per release (CLI flags, build steps, version pins) lives in the main repo's README / DEVELOPMENT.md instead.
