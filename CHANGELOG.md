# UNRELEASED

# 0.1.0 (April 22nd, 2026)

FEATURES

1. SIMD-accelerated byte-class scanning for lexers and parsers ([#1])
   - Built-in ASCII class functions: `skip_binary`, `skip_octal_digits`, `skip_digits`,
     `skip_hex_digits`, `skip_alpha`, `skip_alphanumeric`, `skip_ident_start`, `skip_ident`,
     `skip_whitespace`, `skip_lower`, `skip_upper`, `skip_ascii`, `skip_non_ascii`,
     `skip_ascii_graphic`, `skip_ascii_control`
   - Generic multi-needle operations: `skip_while`, `skip_until`, `count_matches`, `find_last`
   - `skip_class!` macro for defining custom byte classes with the same SIMD dispatch as built-ins
   - `Needles` trait accepting `u8`, `[u8; N]`, and `&[u8]`
   - Runtime dispatch across AVX-512BW → AVX2 → SSE4.2 → scalar on x86/x86\_64,
     NEON → scalar on aarch64, SIMD128 → scalar on wasm32
   - Zero-allocation, `no_std`-compatible

[#1]: https://github.com/al8n/memspan/pull/1
