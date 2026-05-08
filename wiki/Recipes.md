# Recipes

Practical, copy-pasteable workflows for common tasks. All examples assume `hhead` is on your `$PATH`.

## Inspect the header of any file

```bash
hhead --input <FILE> --meta --width 32 --bytes 256
```

`--meta` prints size, timestamps, permissions, detected format, and any format-specific fields. `--bytes` keeps the dump short while still covering most container headers.

---

## Triage a "corrupt" PNG

```bash
hhead --input suspect.png --meta --bytes 64 --width 16
```

What to look for:

- `Format: PNG` confirms the magic bytes (`89 50 4e 47 0d 0a 1a 0a`) are intact.
- `Dimensions`, `Bit depth`, `Color type` come from the IHDR chunk.
- If `Format:` is missing or wrong, the magic bytes themselves are damaged — the file likely isn't a PNG at all.
- If `Format: PNG` is present but `Dimensions` is missing, the IHDR chunk identifier is wrong (we explicitly check `IHDR` at offset 12).

---

## See what's inside a ZIP without unzipping

```bash
hhead --input archive.zip --meta --bytes 512 --width 32
```

The metadata block tells you:

- Compression method (`Stored`, `Deflated`, …).
- Compressed and uncompressed size of the **first** entry.
- The first entry's filename.

The hex column will also show the local file header (`PK\x03\x04`) and, for small archives, the central directory record (`PK\x01\x02`) further down.

---

## Read a GZIP modification time

```bash
hhead --input file.gz --meta --bytes 16
```

The `Modification time` field is the original mtime baked into the GZIP header — useful when the on-disk mtime has been clobbered by `cp` / `tar` / a download.

---

## Spot a UTF-8 BOM

```bash
hhead --input doc.txt --width 16 --bytes 16
```

A UTF-8 BOM is `EF BB BF` at offset 0. If the file *should* be plain UTF-8, that's three bytes of garbage at the start of the file.

```
00000000: ef bb bf 48 65 6c 6c 6f  ...                       |...Hello...
```

---

## Render an image as a terminal minimap

```bash
hhead --input photo.jpg --minimap --minimap-scale 16x40 --color
```

`--minimap-scale ROWSxCOLS` controls the thumbnail grid. Larger grids show more detail but require a wider terminal. Add `--meta` to see decoded dimensions next to the minimap.

For a quick "what does this image look like" check on a remote server over SSH:

```bash
hhead --input /var/www/uploads/123.png --minimap --minimap-scale 24x80 --color
```

---

## Find the JPEG dimensions of a stripped-EXIF image

```bash
hhead --input photo.jpg --meta --bytes 1024
```

`hhead` walks the JPEG segments looking for an SOFn frame marker — that's where the real pixel dimensions live. Even after `exiftool -all=` strips metadata, the SOF segment remains, so dimensions are still reported.

---

## Diff two binaries, byte for byte

`hhead` makes a great pre-stage for `diff`:

```bash
hhead --input a.bin --width 16 --bytes 1024 > a.hex
hhead --input b.bin --width 16 --bytes 1024 > b.hex
diff -u a.hex b.hex
```

The fixed `--width` ensures rows line up on both sides.

---

## Pipe-friendly: capture a dump for a bug report

```bash
hhead --input mystery.bin --meta --width 16 --bytes 256 | tee dump.txt
```

`hhead` doesn't emit colors when stdout isn't a TTY (the `colored` crate auto-detects), so `dump.txt` stays plain text and is safe to paste into an issue.

If you *want* colors in a piped output (e.g. for `less -R`), add `--color` to override the auto-detection.

---

## Larger-than-4-GiB files

```bash
hhead --input bigdump.iso --bytes 64 --width 32
```

Offsets widen to 16 hex digits automatically once the data is past 4 GiB — no flag required. Note `--bytes` caps how much `hhead` reads, so you'll usually combine this with seek-style tools (e.g. `dd if=bigdump.iso bs=1M skip=2048 | head -c 4096 | hhead --input /dev/stdin`).

---

## See full help

```bash
hhead --help
hhead --version
```
