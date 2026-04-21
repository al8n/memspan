<div align="center">
<h1>memspan</h1>
</div>
<div align="center">

SIMD-accelerated byte-class scanning for lexers and parsers.

[<img alt="github" src="https://img.shields.io/badge/github-al8n/memspan-8da0cb?style=for-the-badge&logo=Github" height="22">][Github-url]
<img alt="LoC" src="https://img.shields.io/endpoint?url=https%3A%2F%2Fgist.githubusercontent.com%2Fal8n%2F327b2a8aef9003246e45c6e47fe63937%2Fraw%2Fmemspan" height="22">
[<img alt="Build" src="https://img.shields.io/github/actions/workflow/status/al8n/memspan/ci.yml?logo=Github-Actions&style=for-the-badge" height="22">][CI-url]
[<img alt="codecov" src="https://img.shields.io/codecov/c/gh/al8n/memspan?style=for-the-badge&token=6R3QFWRWHL&logo=codecov" height="22">][codecov-url]

[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-memspan-66c2a5?style=for-the-badge&labelColor=555555&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMiA0MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMiAzOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMiA0MS40di0uNmwxMDIgMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">][doc-url]
[<img alt="crates.io" src="https://img.shields.io/crates/v/memspan?style=for-the-badge&logo=data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0iaXNvLTg4NTktMSI/Pg0KPCEtLSBHZW5lcmF0b3I6IEFkb2JlIElsbHVzdHJhdG9yIDE5LjAuMCwgU1ZHIEV4cG9ydCBQbHVnLUluIC4gU1ZHIFZlcnNpb246IDYuMDAgQnVpbGQgMCkgIC0tPg0KPHN2ZyB2ZXJzaW9uPSIxLjEiIGlkPSJMYXllcl8xIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIiB4PSIwcHgiIHk9IjBweCINCgkgdmlld0JveD0iMCAwIDUxMiA1MTIiIHhtbDpzcGFjZT0icHJlc2VydmUiPg0KPGc+DQoJPGc+DQoJCTxwYXRoIGQ9Ik0yNTYsMEwzMS41MjgsMTEyLjIzNnYyODcuNTI4TDI1Niw1MTJsMjI0LjQ3Mi0xMTIuMjM2VjExMi4yMzZMMjU2LDB6IE0yMzQuMjc3LDQ1Mi41NjRMNzQuOTc0LDM3Mi45MTNWMTYwLjgxDQoJCQlsMTU5LjMwMyw3OS42NTFWNDUyLjU2NHogTTEwMS44MjYsMTI1LjY2MkwyNTYsNDguNTc2bDE1NC4xNzQsNzcuMDg3TDI1NiwyMDIuNzQ5TDEwMS44MjYsMTI1LjY2MnogTTQzNy4wMjYsMzcyLjkxMw0KCQkJbC0xNTkuMzAzLDc5LjY1MVYyNDAuNDYxbDE1OS4zMDMtNzkuNjUxVjM3Mi45MTN6IiBmaWxsPSIjRkZGIi8+DQoJPC9nPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPC9zdmc+DQo=" height="22">][crates-url]
[<img alt="crates.io" src="https://img.shields.io/crates/d/memspan?color=critical&logo=data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBzdGFuZGFsb25lPSJubyI/PjwhRE9DVFlQRSBzdmcgUFVCTElDICItLy9XM0MvL0RURCBTVkcgMS4xLy9FTiIgImh0dHA6Ly93d3cudzMub3JnL0dyYXBoaWNzL1NWRy8xLjEvRFREL3N2ZzExLmR0ZCI+PHN2ZyB0PSIxNjQ1MTE3MzMyOTU5IiBjbGFzcz0iaWNvbiIgdmlld0JveD0iMCAwIDEwMjQgMTAyNCIgdmVyc2lvbj0iMS4xIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHAtaWQ9IjM0MjEiIGRhdGEtc3BtLWFuY2hvci1pZD0iYTMxM3guNzc4MTA2OS4wLmkzIiB3aWR0aD0iNDgiIGhlaWdodD0iNDgiIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIj48ZGVmcz48c3R5bGUgdHlwZT0idGV4dC9jc3MiPjwvc3R5bGU+PC9kZWZzPjxwYXRoIGQ9Ik00NjkuMzEyIDU3MC4yNHYtMjU2aDg1LjM3NnYyNTZoMTI4TDUxMiA3NTYuMjg4IDM0MS4zMTIgNTcwLjI0aDEyOHpNMTAyNCA2NDAuMTI4QzEwMjQgNzgyLjkxMiA5MTkuODcyIDg5NiA3ODcuNjQ4IDg5NmgtNTEyQzEyMy45MDQgODk2IDAgNzYxLjYgMCA1OTcuNTA0IDAgNDUxLjk2OCA5NC42NTYgMzMxLjUyIDIyNi40MzIgMzAyLjk3NiAyODQuMTYgMTk1LjQ1NiAzOTEuODA4IDEyOCA1MTIgMTI4YzE1Mi4zMiAwIDI4Mi4xMTIgMTA4LjQxNiAzMjMuMzkyIDI2MS4xMkM5NDEuODg4IDQxMy40NCAxMDI0IDUxOS4wNCAxMDI0IDY0MC4xOTJ6IG0tMjU5LjItMjA1LjMxMmMtMjQuNDQ4LTEyOS4wMjQtMTI4Ljg5Ni0yMjIuNzItMjUyLjgtMjIyLjcyLTk3LjI4IDAtMTgzLjA0IDU3LjM0NC0yMjQuNjQgMTQ3LjQ1NmwtOS4yOCAyMC4yMjQtMjAuOTI4IDIuOTQ0Yy0xMDMuMzYgMTQuNC0xNzguMzY4IDEwNC4zMi0xNzguMzY4IDIxNC43MiAwIDExNy45NTIgODguODMyIDIxNC40IDE5Ni45MjggMjE0LjRoNTEyYzg4LjMyIDAgMTU3LjUwNC03NS4xMzYgMTU3LjUwNC0xNzEuNzEyIDAtODguMDY0LTY1LjkyLTE2NC45MjgtMTQ0Ljk2LTE3MS43NzZsLTI5LjUwNC0yLjU2LTUuODg4LTMwLjk3NnoiIGZpbGw9IiNmZmZmZmYiIHAtaWQ9IjM0MjIiIGRhdGEtc3BtLWFuY2hvci1pZD0iYTMxM3guNzc4MTA2OS4wLmkwIiBjbGFzcz0iIj48L3BhdGg+PC9zdmc+&style=for-the-badge" height="22">][crates-url]
<img alt="license" src="https://img.shields.io/badge/License-Apache%202.0/MIT-blue.svg?style=for-the-badge&fontColor=white&logoColor=f5c076&logo=data:image/svg+xml;base64,PCFET0NUWVBFIHN2ZyBQVUJMSUMgIi0vL1czQy8vRFREIFNWRyAxLjEvL0VOIiAiaHR0cDovL3d3dy53My5vcmcvR3JhcGhpY3MvU1ZHLzEuMS9EVEQvc3ZnMTEuZHRkIj4KDTwhLS0gVXBsb2FkZWQgdG86IFNWRyBSZXBvLCB3d3cuc3ZncmVwby5jb20sIFRyYW5zZm9ybWVkIGJ5OiBTVkcgUmVwbyBNaXhlciBUb29scyAtLT4KPHN2ZyBmaWxsPSIjZmZmZmZmIiBoZWlnaHQ9IjgwMHB4IiB3aWR0aD0iODAwcHgiIHZlcnNpb249IjEuMSIgaWQ9IkNhcGFfMSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIiB4bWxuczp4bGluaz0iaHR0cDovL3d3dy53My5vcmcvMTk5OS94bGluayIgdmlld0JveD0iMCAwIDI3Ni43MTUgMjc2LjcxNSIgeG1sOnNwYWNlPSJwcmVzZXJ2ZSIgc3Ryb2tlPSIjZmZmZmZmIj4KDTxnIGlkPSJTVkdSZXBvX2JnQ2FycmllciIgc3Ryb2tlLXdpZHRoPSIwIi8+Cg08ZyBpZD0iU1ZHUmVwb190cmFjZXJDYXJyaWVyIiBzdHJva2UtbGluZWNhcD0icm91bmQiIHN0cm9rZS1saW5lam9pbj0icm91bmQiLz4KDTxnIGlkPSJTVkdSZXBvX2ljb25DYXJyaWVyIj4gPGc+IDxwYXRoIGQ9Ik0xMzguMzU3LDBDNjIuMDY2LDAsMCw2Mi4wNjYsMCwxMzguMzU3czYyLjA2NiwxMzguMzU3LDEzOC4zNTcsMTM4LjM1N3MxMzguMzU3LTYyLjA2NiwxMzguMzU3LTEzOC4zNTcgUzIxNC42NDgsMCwxMzguMzU3LDB6IE0xMzguMzU3LDI1OC43MTVDNzEuOTkyLDI1OC43MTUsMTgsMjA0LjcyMywxOCwxMzguMzU3UzcxLjk5MiwxOCwxMzguMzU3LDE4IHMxMjAuMzU3LDUzLjk5MiwxMjAuMzU3LDEyMC4zNTdTMjA0LjcyMywyNTguNzE1LDEzOC4zNTcsMjU4LjcxNXoiLz4gPHBhdGggZD0iTTE5NC43OTgsMTYwLjkwM2MtNC4xODgtMi42NzctOS43NTMtMS40NTQtMTIuNDMyLDIuNzMyYy04LjY5NCwxMy41OTMtMjMuNTAzLDIxLjcwOC0zOS42MTQsMjEuNzA4IGMtMjUuOTA4LDAtNDYuOTg1LTIxLjA3OC00Ni45ODUtNDYuOTg2czIxLjA3Ny00Ni45ODYsNDYuOTg1LTQ2Ljk4NmMxNS42MzMsMCwzMC4yLDcuNzQ3LDM4Ljk2OCwyMC43MjMgYzIuNzgyLDQuMTE3LDguMzc1LDUuMjAxLDEyLjQ5NiwyLjQxOGM0LjExOC0yLjc4Miw1LjIwMS04LjM3NywyLjQxOC0xMi40OTZjLTEyLjExOC0xNy45MzctMzIuMjYyLTI4LjY0NS01My44ODItMjguNjQ1IGMtMzUuODMzLDAtNjQuOTg1LDI5LjE1Mi02NC45ODUsNjQuOTg2czI5LjE1Miw2NC45ODYsNjQuOTg1LDY0Ljk4NmMyMi4yODEsMCw0Mi43NTktMTEuMjE4LDU0Ljc3OC0zMC4wMDkgQzIwMC4yMDgsMTY5LjE0NywxOTguOTg1LDE2My41ODIsMTk0Ljc5OCwxNjAuOTAzeiIvPiA8L2c+IDwvZz4KDTwvc3ZnPg==" height="22">

English | [简体中文][zh-cn-url]

</div>

## Quick start

```rust
let src = b"   hello, world";

// Skip leading whitespace — dispatches to AVX2 / NEON / SIMD128 at runtime.
let n = memspan::skip_whitespace(src);
assert_eq!(n, 3);

// Find the first comma.
let comma = memspan::skip_until(src, b',');
assert_eq!(comma, Some(8));
```

## Overview

`memspan` provides zero-allocation, `no_std`-compatible functions to skip,
count, and locate bytes in ASCII character classes, dispatching to the best
available SIMD backend at runtime:

| Architecture | Dispatch order |
|---|---|
| x86\_64 | AVX-512BW → AVX2 → SSE4.2 → scalar |
| x86 | SSE4.2 → scalar |
| aarch64 | NEON → scalar |
| wasm32 | SIMD128 → scalar |
| other | scalar |

## Installation

```toml
[dependencies]
memspan = "0.1"
```

For `no_std` without an allocator:

```toml
[dependencies]
memspan = { version = "0.1", default-features = false }
```

## Built-in classes

All class functions return the byte length of the longest matching prefix.

| Function | Matches |
|---|---|
| `skip_whitespace` | ` `, `\t`, `\r`, `\n` |
| `skip_digits` | `0`–`9` |
| `skip_hex_digits` | `0`–`9`, `a`–`f`, `A`–`F` |
| `skip_octal_digits` | `0`–`7` |
| `skip_binary` | `0`, `1` |
| `skip_alpha` | `a`–`z`, `A`–`Z` |
| `skip_alphanumeric` | `a`–`z`, `A`–`Z`, `0`–`9` |
| `skip_ident_start` | `a`–`z`, `A`–`Z`, `_` |
| `skip_ident` | `a`–`z`, `A`–`Z`, `0`–`9`, `_` |
| `skip_lower` | `a`–`z` |
| `skip_upper` | `A`–`Z` |
| `skip_ascii` | `0x00`–`0x7F` |
| `skip_non_ascii` | `0x80`–`0xFF` |
| `skip_ascii_graphic` | `0x21`–`0x7E` (printable non-space) |
| `skip_ascii_control` | `0x00`–`0x1F`, `0x7F` |

## Generic operations

`skip_while`, `skip_until`, `count_matches`, and `find_last` accept any
[`Needles`] value — a single `u8`, a fixed-size array `[u8; N]`, or a `&[u8]`
slice:

```rust
// Skip while any of several bytes match.
let n = memspan::skip_while(b"  ,\t ok", [b' ', b',', b'\t']);
assert_eq!(n, 5);

// Find the first occurrence of any needle — like memchr but multi-byte.
let pos = memspan::skip_until(b"hello\nworld", b'\n');
assert_eq!(pos, Some(5));

// Count every newline in a buffer.
let lines = memspan::count_matches(b"a\nb\nc\n", b'\n');
assert_eq!(lines, 3);

// Find the rightmost match.
let last = memspan::find_last(b"\"hello\"", b'"');
assert_eq!(last, Some(6));
```

## Custom classes with `skip_class!`

Define your own byte class and get the same SIMD dispatch as the built-ins:

```rust
memspan::skip_class! {
    /// Skip whitespace and commas.
    pub fn skip_ws_and_comma(bytes = [b' ', b'\t', b'\r', b'\n', b',']);
}

memspan::skip_class! {
    /// Skip lowercase ASCII letters.
    pub fn skip_lowercase(ranges = [b'a'..=b'z']);
}

memspan::skip_class! {
    /// Skip alphanumeric plus common punctuation.
    pub fn skip_punct_ident(
        bytes  = [b'_', b'-', b'!', b'?'],
        ranges = [b'a'..=b'z', b'A'..=b'Z', b'0'..=b'9'],
    );
}

assert_eq!(skip_ws_and_comma(b"  , ok"), 4);
assert_eq!(skip_lowercase(b"abcXYZ"), 3);
assert_eq!(skip_punct_ident(b"hello-world! 42"), 12);
```

## Benchmarks

Throughput at 4096-byte input, measured on GitHub Actions runners (2026-04-21).

### Built-in ASCII class functions

| Function | aarch64 NEON | aarch64 scalar | x86\_64 AVX2 | x86\_64 scalar |
|---|---|---|---|---|
| `skip_binary` | 24.8 GiB/s | 2.4 GiB/s | 59.1 GiB/s | 3.0 GiB/s |
| `skip_octal_digits` | 39.9 GiB/s | 2.4 GiB/s | 62.1 GiB/s | 3.0 GiB/s |
| `skip_digits` | 39.3 GiB/s | 2.5 GiB/s | 63.4 GiB/s | 3.0 GiB/s |
| `skip_hex_digits` | 21.7 GiB/s | 1.4 GiB/s | 33.2 GiB/s | 1.5 GiB/s |
| `skip_alpha` | 35.1 GiB/s | 1.9 GiB/s | 62.4 GiB/s | 1.8 GiB/s |
| `skip_alphanumeric` | 21.7 GiB/s | 1.6 GiB/s | 31.9 GiB/s | 1.5 GiB/s |
| `skip_ident_start` | 23.9 GiB/s | 1.7 GiB/s | 40.0 GiB/s | 1.5 GiB/s |
| `skip_ident` | 17.8 GiB/s | 1.3 GiB/s | 28.5 GiB/s | 1.5 GiB/s |
| `skip_whitespace` | 16.5 GiB/s | 1.5 GiB/s | 38.4 GiB/s | 2.0 GiB/s |

### Generic operations

| Function | aarch64 NEON | aarch64 scalar | x86\_64 AVX2 | x86\_64 scalar |
|---|---|---|---|---|
| `skip_until` | 12.9 GiB/s | 1.5 GiB/s | 26.3 GiB/s | 1.0 GiB/s |
| `skip_while` | 15.2 GiB/s | 2.0 GiB/s | 26.2 GiB/s | 1.2 GiB/s |

### `skip_class!` macro vs `skip_while`

| | aarch64 NEON | x86\_64 AVX2 |
|---|---|---|
| `skip_class!` | 14.2 GiB/s | 36.0 GiB/s |
| `skip_while` (array) | 15.1 GiB/s | 26.8 GiB/s |

**Environments:** aarch64 — macOS-latest runner (ARM64, NEON); x86\_64 — ubuntu-latest runner (X64, runtime AVX2 detection). All runs with `RUSTFLAGS=""`.

## Features

| Feature | Default | Description |
|---|---|---|
| `std` | ✓ | Link against the standard library |
| `alloc` | | Enable heap allocation without `std` |
| _(neither)_ | | Pure `no_std` / `no_alloc` |

#### License

`memspan` is dual-licensed under the MIT license and the Apache License (Version 2.0).

See [LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT) for details.

Copyright (c) 2026 Al Liu.

[Github-url]: https://github.com/al8n/memspan/
[CI-url]: https://github.com/al8n/memspan/actions/workflows/ci.yml
[doc-url]: https://docs.rs/memspan
[crates-url]: https://crates.io/crates/memspan
[codecov-url]: https://app.codecov.io/gh/al8n/memspan/
[zh-cn-url]: https://github.com/al8n/memspan/tree/main/README-zh_CN.md
