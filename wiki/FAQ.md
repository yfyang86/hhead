# FAQ

## Usage

### Why do I need `--input`? Can `hhead` read from stdin?

Today the CLI requires `--input <FILE>`. Reading from stdin is a reasonable feature request — track it in [issues](https://github.com/yfyang86/hhead/issues). Workaround:

```bash
some-pipeline | tee /tmp/buf.bin >/dev/null && hhead --input /tmp/buf.bin
```

### Why are colors missing when I pipe to `less`?

The `colored` crate auto-disables ANSI when stdout isn't a TTY. Force colors on with `--color`, then read with `less -R`:

```bash
hhead --input file.bin --color | less -R
```

### Why is the right-hand `|` column misaligned for Chinese / Japanese / emoji text?

In `--utf8` mode the character column counts Unicode *characters*, but wide CJK and emoji glyphs occupy 2 terminal cells. There's no portable way to know cell width without integrating a Unicode-width library — the trailing `|` is only strictly aligned for ASCII input. Tracked as a known limitation.

### How do I dump *all* of a file?

`hhead` defaults to the first 256 bytes — it's a *head* utility, not a full dumper. Pass `--bytes <N>` for more. For genuinely large dumps, prefer `xxd` or `od`; `hhead`'s niche is "show me the first interesting kilobyte and what format it is."

### Does `--bytes` honor file boundaries?

Yes. If the file is shorter than `--bytes`, `hhead` reads everything and stops. If it's longer, only the first `--bytes` are read.

### Can I change the byte grouping (8 bytes per block)?

Not via a flag today; the grouping is fixed at 8. `--width` controls bytes-per-line, which indirectly controls how many groups appear per row.

## Output

### What do the columns mean?

```
00000000: 48 65 6c 6c 6f 20 57 6f  72 6c 64 21              |Hello World!    |
└─offset: └─ hex bytes (groups of 8)─┘                       └─ ASCII column ─┘
```

- **Offset.** 8 hex digits (16 for files past 4 GiB). Marks the byte index of the first byte on this row.
- **Hex bytes.** Each byte in lowercase hex, grouped 8 per block.
- **ASCII column.** Printable ASCII or, with `--utf8`, decoded UTF-8. Non-printable bytes show as `.`.

### What does `Format: …` mean under `--meta`?

`hhead` peeks at the first 1024 bytes and tries to match a known magic-byte signature (PNG, JPEG, GIF, BMP, ZIP, GZIP, TAR, TIFF, PDF). If a format is recognized, format-specific fields follow. See [Format-Internals](./Format-Internals).

### Why does my BMP show no extra metadata?

`hhead` only parses BMPs that use the modern `BITMAPINFOHEADER` (size ≥ 40 at offset 14). Older `BITMAPCOREHEADER` files fall back to "format detected, no fields."

### Why is the timestamp printed as a Unix integer?

Keeps output stable across locales and avoids pulling in a date-formatting dependency. To convert quickly:

```bash
date -d @1769525204
```

(macOS: `date -r 1769525204`.)

## Performance

### Is `hhead` slow on big files?

For the default `--bytes 256` it's effectively instant — only the head of the file is ever read. The whole-file allocation is bounded by `--bytes`.

For minimap on large images, the `image` crate decodes the full picture before sampling. Multi-megabyte JPEGs may take a noticeable beat.

### Why is colored output noticeably slower than plain?

The `colored` crate creates a heap-allocated `String` per styled token. For interactive use this is invisible; for `--bytes` ≥ a few MiB it adds up. Plain output writes via a locked stdout writer with no per-row allocation.

## Troubleshooting

### "Error: File 'foo' not found"

Path is relative to your shell's current directory, not the binary's location. Use absolute paths or `pwd` to check.

### "Warning: Minimap failed: …"

The file's magic bytes claimed it was an image format `image` could read, but decode failed (truncated file, unusual subformat, broken filters). The hex dump still proceeds.

### "Warning: Invalid minimap scale format 'X'"

`--minimap-scale` expects exactly `ROWSxCOLS`, both positive integers. Examples: `8x12`, `32x80`, `1x1`.

### `hhead --help` shows fewer flags than I expect

You may be on an older release. Run `hhead --version` and compare with the CHANGELOG / [latest release](https://github.com/yfyang86/hhead/releases).

## Compatibility

### Supported platforms?

Linux, macOS, and Windows all build. Unix gets octal permission output (e.g. `0644`); other platforms see `read-only` / `read-write`.

### Minimum Rust version?

Rust 1.85+ — the crate is on edition 2024.

### Is `hhead` a `xxd` / `hexdump` replacement?

No — it's a complementary tool focused on the *first kilobyte* of a file, with format awareness and an image minimap. For full-file dumps, scripted diff workflows, or strict POSIX behavior, prefer `xxd` or `od`.
