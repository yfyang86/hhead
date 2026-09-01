# Developing hhead

This document is for contributors. For end-user installation and usage, see [Readme.md](./Readme.md).

## Prerequisites

- **Rust 1.88+** — `anydoc`'s MSRV (the crate itself is on `edition = "2024"`). Install via [rustup](https://rustup.rs/).
- `cargo` (ships with rustup).
- A Unix-like OS is recommended; the crate also builds on Windows but permission formatting differs.

Check your toolchain:

```bash
rustc --version   # >= 1.85
cargo --version
```

## Clone and build

```bash
git clone https://github.com/yfyang86/hhead
cd hhead
cargo build           # debug build
cargo build --release # optimized; binary at target/release/hhead
```

## Running and testing

```bash
# unit tests (lib) + integration tests (tests/)
cargo test

# run a specific test
cargo test --lib display::hex::tests::test_display_hex_basic

# lint / compile check
cargo check

# run against a file without installing
cargo run -- --input Cargo.toml --meta --width 32
```

The test suite currently covers 71 unit tests (format detection, metadata extraction, hex formatting, Markdown rendering, color palette, argument parsing, pager helpers) and 21 integration tests that drive the CLI end-to-end via `assert_cmd`.

## Project layout

```
hhead/
├── Cargo.toml                  # crate manifest
├── Readme.md                   # user-facing docs
├── DEVELOPMENT.md              # this file
├── LICENSE
├── assets/                     # images used by the README
├── src/
│   ├── lib.rs                  # library root; re-exports the module tree
│   ├── main.rs                 # binary entry point (argument parsing + glue)
│   ├── cli/
│   │   ├── mod.rs
│   │   └── args.rs             # `Args` (clap derive) + validation
│   ├── io/
│   │   ├── mod.rs
│   │   └── file.rs             # bounded file reader
│   ├── formats/
│   │   ├── mod.rs
│   │   ├── detection.rs        # magic-byte → format-name lookup
│   │   └── metadata.rs         # format-specific metadata extractors
│   ├── display/
│   │   ├── mod.rs
│   │   ├── hex.rs              # `display_hex` / `write_hex<W: Write>`
│   │   ├── markdown.rs         # `display_markdown` / `write_markdown<W: Write>` terminal renderer
│   │   ├── metadata.rs         # `print_metadata` / `write_metadata<W: Write>`
│   │   ├── minimap.rs          # 256-color image thumbnail renderer
│   │   ├── pager.rs            # `run_pager`: built-in less-style pager + pure helpers
│   │   └── tree.rs             # directory mode: tree listing + ls/du-style meta block
│   └── utils/
│       ├── mod.rs
│       ├── color.rs            # RGB → xterm-256 palette index
│       └── parsing.rs          # `parse_scale("ROWSxCOLS")`
└── tests/
    └── integration_tests.rs    # CLI-level tests via assert_cmd
```

The crate is split `lib` + `bin`: `main.rs` is a thin driver that wires up modules exposed from `lib.rs`. All logic lives in the library so it can be unit-tested in isolation.

## Architecture at a glance

```
args (clap)  ──▶  main.rs
                   │
                   ├── io::read_file        (bounded read into Vec<u8>)
                   ├── formats::detection   (magic-byte → &'static str)
                   ├── formats::metadata    (format → Vec<String> fields)
                   └── display::{hex, markdown, metadata, minimap}
                                 │
                                 └── utils::{color, parsing}
```

Design notes:

- **Library-first.** `src/main.rs` should stay small; new functionality lives in `src/<area>/` and is re-exported through `mod.rs`.
- **I/O at the edges.** Format parsers (`formats/`) take a `&[u8]` so they're trivially testable without touching the filesystem.
- **`display::hex::write_hex`** takes `&mut impl Write`, so tests capture output into a `Vec<u8>` and assert on the exact bytes. `display_hex` is a thin wrapper that locks `stdout` once for atomic output. When adding new display functions, follow the same pattern.
- **No panics on malformed input.** Format parsers must bounds-check every index. Use explicit length guards *and* identity checks (e.g. confirm chunk tags) before reading structured fields.
- **The pager (`display::pager`)** owns the only terminal I/O beyond plain stdout: raw mode + alternate screen via `crossterm` (added for this feature; it is the one place a terminal library is justified). Pure helpers (`visible_width`, `truncate_ansi`, `find_matches`, `clamp_offset`) are separated out and unit-tested. When stdin/stdout is not a TTY, `run_pager` falls back to dumping the content so piped use stays deterministic.
- **`--mode-anydoc`** is thin glue in `main.rs`: `markdown_source` runs `anydoc::to_markdown_bytes` (content detection with extension fallback for CSV) and hands the result to the same `write_markdown` renderer `--markdown` uses. On conversion failure it warns and falls back — text input renders as Markdown, anything else as a hex dump. `anydoc`'s conversion needs the `log` facade, which is a no-op without a logger; no logging setup is required.

## Adding a new file-format parser

1. Extend `detect_file_format` in `src/formats/detection.rs` with the magic-byte signature. Return a stable `&'static str` tag.
2. Add a unit test to `src/formats/detection.rs`.
3. Add a matching arm in `extract_format_metadata` in `src/formats/metadata.rs`. Guard every index against `data.len()`.
4. Add a unit test that builds a minimal fixture as `Vec<u8>` and asserts the output contents.
5. Document the format in the `--meta` table in [Readme.md](./Readme.md).

## Coding conventions

- **Formatting.** `cargo fmt` before committing.
- **Lints.** `cargo clippy --all-targets` should be clean; prefer fixing over `#[allow]` unless the warning is spurious.
- **Comments.** Only when the *why* is non-obvious — a subtle invariant, a spec quirk, a workaround. Identifiers describe the *what*.
- **Errors.** Use `io::Result` at I/O boundaries; `io::Error::other(msg)` to wrap foreign errors rather than `io::Error::new(ErrorKind::Other, …)`.
- **No new dependencies** without a reason. The current deps are `clap`, `colored`, `image`, `crossterm` (interactive pager), and `anydoc` (`--mode-anydoc` document conversion); additions should be discussed in the PR.

## Running the binary locally

```bash
# dump this file
cargo run -- --input Readme.md --width 48

# force color + metadata against an image
cargo run -- --input assets/main.gif --color --meta --width 32

# try the minimap
cargo run -- --input assets/main.gif --minimap --minimap-scale 16x40

# render markdown (tables aligned, figures drawn as minimaps)
cargo run -- --input Readme.md --markdown --color

# page through a big file
cargo run -- --input Cargo.toml --mode-less --color --meta
```

## Release build

```bash
cargo build --release
./target/release/hhead --version
```

Strip debug info for a smaller binary (optional):

```bash
strip target/release/hhead
```

## Submitting changes

1. Fork and create a feature branch off `main`.
2. Write or update tests alongside the change — unit tests for pure functions in `src/`, integration tests in `tests/integration_tests.rs` for CLI-visible behavior.
3. Run `cargo fmt`, `cargo clippy --all-targets`, and `cargo test` locally.
4. Keep commits focused; prefer several small commits over one large squash.
5. Open a pull request with a short description of the change, the motivation, and the test plan.

## Known limitations & good first issues

- `assert_cmd::Command::cargo_bin` is deprecated in our pinned 2.0 dev-dependency; bumping it will surface new APIs (`cargo::cargo_bin_cmd!`).
- UTF-8 character column in `display::hex` doesn't account for terminal cell width of CJK / emoji characters — alignment drifts in that case. Fix ideas: integrate `unicode-width`, or chunk along char boundaries.
- `detect_file_format` returns a stringly-typed tag consumed by `extract_format_metadata`. Converting it into an `enum FileFormat { … }` would remove the string coupling between the two modules.
- Only PNG / JPEG / BMP / GIF metadata currently has round-trip unit tests; parsers for ZIP / GZIP / TAR / TIFF / PDF would benefit from fixture-based tests too.
- The pager's search is line-based and case-insensitive with no match highlighting, and the pager loop itself has no unit tests (only the pure helpers do) — exercising it needs a pty harness.
- `--mode-less` reads the whole file into memory to page it; hex-dumping a multi-GB file through the pager is therefore memory-hungry (the output buffer is ~4.5× the input).
- `anydoc` pulls a heavy dependency tree (calamine, pdf-inspector, …), which noticeably lengthens first builds and raises the MSRV to 1.88. The `--mode-anydoc` fallback heuristic is intentionally simple: valid-UTF-8 input that `anydoc` rejects is treated as Markdown.
