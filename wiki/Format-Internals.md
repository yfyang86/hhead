# Format Internals

What `hhead --meta` actually inspects, byte by byte. Useful when you want to reason about the output, debug a misdetection, or correlate the metadata fields with the hex column.

All offsets are zero-based and refer to the first 1024 bytes that `hhead` reads from the file.

---

## PNG — `\x89PNG\r\n\x1a\n`

Layout:

| Offset | Length | Field |
|---|---|---|
| 0 | 8 | Signature `89 50 4E 47 0D 0A 1A 0A` |
| 8 | 4 | IHDR length (always 13, big-endian) |
| 12 | 4 | Chunk type `"IHDR"` |
| 16 | 4 | Width (big-endian u32) |
| 20 | 4 | Height (big-endian u32) |
| 24 | 1 | Bit depth |
| 25 | 1 | Color type |

Color type values:

| Code | Meaning |
|---|---|
| 0 | Grayscale |
| 2 | RGB |
| 3 | Indexed |
| 4 | Grayscale + Alpha |
| 6 | RGB + Alpha |

`hhead` requires ≥ 26 bytes **and** verifies the chunk identifier at offset 12 is `"IHDR"` before reporting any fields. A non-IHDR first chunk yields no metadata.

---

## BMP — `BM`

`hhead` only parses files that use `BITMAPINFOHEADER` (header size ≥ 40 at offset 14). Earlier `BITMAPCOREHEADER` files are recognized as BMP but produce no extra fields.

| Offset | Length | Field |
|---|---|---|
| 0 | 2 | Signature `"BM"` |
| 14 | 4 | Header size (must be ≥ 40, little-endian) |
| 18 | 4 | Width (i32, little-endian) |
| 22 | 4 | Height (i32, little-endian; negative = top-down DIB) |
| 28 | 2 | Bits per pixel (u16) |
| 30 | 4 | Compression (u32) |

Compression values:

| Code | Name |
|---|---|
| 0 | BI_RGB |
| 1 | BI_RLE8 |
| 2 | BI_RLE4 |
| 3 | BI_BITFIELDS |
| 4 | BI_JPEG |
| 5 | BI_PNG |

The `Orientation` field reports `Top-down` if the height i32 is negative, `Bottom-up` otherwise.

---

## JPEG — `\xff\xd8\xff`

JPEG is a stream of `0xFF`-prefixed markers. `hhead` walks segments looking for an SOFn (Start Of Frame) marker, which carries pixel dimensions. The walker:

1. Starts after the SOI (`FF D8`) at offset 2.
2. For each marker:
   - **SOFn** (`C0..=CF`, excluding `C4` DHT, `C8` JPG, `CC` DAC) — read precision (1 byte), height (u16 BE), width (u16 BE), components (1 byte).
   - **Standalone markers** (`01`, `D0..=D9`) — no length, advance by 2.
   - **Other markers** — read 2-byte big-endian length, advance by `2 + length`.
3. Bounds-check against `data.len()` at every step.

Reported fields: `Dimensions: W x H` and `Components: N` (1 = grayscale, 3 = YCbCr / RGB, 4 = CMYK).

---

## GIF — `GIF87a` / `GIF89a`

| Offset | Length | Field |
|---|---|---|
| 0 | 6 | `"GIF87a"` or `"GIF89a"` |
| 6 | 2 | Width (u16, little-endian) |
| 8 | 2 | Height (u16, little-endian) |
| 10 | 1 | Packed byte |

Packed byte bits (MSB → LSB):

| Bit | Field |
|---|---|
| 7 | Global Color Table flag |
| 6–4 | Color resolution (value + 1 = bits per primary color) |
| 3 | Sort flag |
| 2–0 | Global Color Table size (`2^(value + 1)` entries) |

---

## ZIP — `PK\x03\x04` / `PK\x05\x06` / `PK\x07\x08`

`hhead` parses the *local file header* of the first entry (signature `PK\x03\x04`):

| Offset | Length | Field |
|---|---|---|
| 0 | 4 | Signature |
| 8 | 2 | Compression method |
| 18 | 4 | Compressed size |
| 22 | 4 | Uncompressed size |
| 26 | 2 | File name length |
| 28 | 2 | Extra field length |
| 30 | name_len | File name (bytes) |

Compression method values:

| Code | Name |
|---|---|
| 0 | Stored |
| 8 | Deflated |
| 9 | Enhanced Deflated |
| 12 | BZIP2 |
| 14 | LZMA |
| 19 | LZ77 |
| 98 | PPMd |

ZIP64, Zip-Crypto, and AES extras aren't surfaced — `hhead` is a header peek, not a full parser.

---

## GZIP — `\x1f\x8b`

| Offset | Length | Field |
|---|---|---|
| 0 | 2 | Signature `1F 8B` |
| 2 | 1 | Compression method |
| 3 | 1 | Flags |
| 4 | 4 | Modification time (u32, little-endian, Unix epoch) |
| 8 | 1 | Extra flags |
| 9 | 1 | Operating system |

Compression method 8 = Deflate (the only one in practice).

OS codes: 0=FAT, 1=Amiga, 2=VMS, 3=Unix, 4=VM/CMS, 5=Atari, 6=HPFS, 7=Macintosh, 8=Z-System, 9=CP/M, 10=TOPS-20, 11=NTFS, 12=QDOS, 13=RISCOS, 255=unknown.

`hhead` suppresses the modification-time line if the field is zero (common when the original mtime wasn't preserved).

---

## TIFF — `II\x2a\x00` (LE) / `MM\x00\x2a` (BE)

| Offset | Length | Field |
|---|---|---|
| 0 | 2 | Byte order — `II` = little-endian, `MM` = big-endian |
| 2 | 2 | Magic `0x002A` |
| 4 | 4 | Offset to first IFD (Image File Directory) |

`hhead` reports endianness and the IFD offset. It does *not* walk the IFD to surface dimensions / compression — full TIFF parsing is intentionally out of scope.

---

## PDF — `%PDF-`

| Offset | Length | Field |
|---|---|---|
| 0 | 5 | `"%PDF-"` |
| 5 | 3 | Version, e.g. `"1.7"` or `"2.0"` |

Reported as `Version: 1.7`.

---

## TAR — USTAR / GNU

`hhead` recognizes a TAR file by the magic at offset 257:

- `ustar\0` → USTAR (POSIX)
- `ustar ` → GNU tar

It then parses the first 512-byte header block:

| Offset | Length | Field |
|---|---|---|
| 0 | 100 | File name |
| 100 | 8 | Mode (octal ASCII) |
| 124 | 12 | Size (octal ASCII) |
| 136 | 12 | Modification time (octal ASCII) |
| 156 | 1 | Type flag |
| 157 | 100 | Link name |

Type flag values:

| Char | Meaning |
|---|---|
| `0` or `\0` | Regular file |
| `1` | Hard link |
| `2` | Symbolic link |
| `3` | Character device |
| `4` | Block device |
| `5` | Directory |
| `6` | FIFO |
| `7` | Contiguous file |

Trailing NUL bytes in the name / linkname fields are stripped before printing. Sizes and times are decoded from octal ASCII and printed as base-10 integers.

---

## How detection actually works

`detect_file_format` (in [`src/formats/detection.rs`](https://github.com/yfyang86/hhead/blob/main/src/formats/detection.rs)) is a flat sequence of byte-prefix checks against the first 1024 bytes. Order matters only for formats with ambiguous prefixes — currently none — but new entries should be added near the bottom to preserve fast paths for the common formats.

Each branch returns a `&'static str` tag that's then matched in [`extract_format_metadata`](https://github.com/yfyang86/hhead/blob/main/src/formats/metadata.rs). Adding a new format is two coordinated edits in those two files plus a new test fixture; see DEVELOPMENT.md for the recipe.
