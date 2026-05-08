# Developing hhead

This document is for contributors. For end-user installation and usage, see [Readme.md](./Readme.md).

## Prerequisites

- **Rust 1.85+** — the crate is on `edition = "2024"`. Install via [rustup](https://rustup.rs/).
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

The test suite currently covers 39 unit tests (format detection, metadata extraction, hex formatting, color palette, argument parsing) and 10 integration tests that drive the CLI end-to-end via `assert_cmd`.

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
│   │   ├── metadata.rs         # `print_metadata`
│   │   └── minimap.rs          # 256-color image thumbnail renderer
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
                   └── display::{hex, metadata, minimap}
                                 │
                                 └── utils::{color, parsing}
```

Design notes:

- **Library-first.** `src/main.rs` should stay small; new functionality lives in `src/<area>/` and is re-exported through `mod.rs`.
- **I/O at the edges.** Format parsers (`formats/`) take a `&[u8]` so they're trivially testable without touching the filesystem.
- **`display::hex::write_hex`** takes `&mut impl Write`, so tests capture output into a `Vec<u8>` and assert on the exact bytes. `display_hex` is a thin wrapper that locks `stdout` once for atomic output. When adding new display functions, follow the same pattern.
- **No panics on malformed input.** Format parsers must bounds-check every index. Use explicit length guards *and* identity checks (e.g. confirm chunk tags) before reading structured fields.

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
- **No new dependencies** without a reason. The current deps are `clap`, `colored`, and `image`; additions should be discussed in the PR.

## Running the binary locally

```bash
# dump this file
cargo run -- --input Readme.md --width 48

# force color + metadata against an image
cargo run -- --input assets/main.gif --color --meta --width 32

# try the minimap
cargo run -- --input assets/main.gif --minimap --minimap-scale 16x40
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
