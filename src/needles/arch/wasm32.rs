use core::arch::wasm32::*;

/// Returns a `v128` where each byte lane is `0xFF` if the byte in `chunk`
/// matches any needle, or `0x00` otherwise.
#[cfg_attr(not(tarpaulin), inline(always))]
pub(in crate::needles) fn eq_any_mask_dynamic_simd128(chunk: v128, needles: &[u8]) -> v128 {
  let mut acc = i8x16_splat(0);
  for &n in needles {
    acc = v128_or(acc, i8x16_eq(chunk, i8x16_splat(n as i8)));
  }
  acc
}

/// Const-dispatch variant: unrolled balanced OR tree for 0–8 needles.
#[cfg_attr(not(tarpaulin), inline(always))]
pub(in crate::needles) fn eq_any_mask_const_simd128<const N: usize>(
  chunk: v128,
  needles: [u8; N],
) -> v128 {
  macro_rules! cmp {
    ($i:expr) => {
      i8x16_eq(chunk, i8x16_splat(needles[$i] as i8))
    };
  }
  match N {
    0 => i8x16_splat(0),
    1 => cmp!(0),
    2 => v128_or(cmp!(0), cmp!(1)),
    3 => v128_or(v128_or(cmp!(0), cmp!(1)), cmp!(2)),
    4 => v128_or(v128_or(cmp!(0), cmp!(1)), v128_or(cmp!(2), cmp!(3))),
    5 => v128_or(
      v128_or(v128_or(cmp!(0), cmp!(1)), v128_or(cmp!(2), cmp!(3))),
      cmp!(4),
    ),
    6 => v128_or(
      v128_or(v128_or(cmp!(0), cmp!(1)), v128_or(cmp!(2), cmp!(3))),
      v128_or(cmp!(4), cmp!(5)),
    ),
    7 => v128_or(
      v128_or(v128_or(cmp!(0), cmp!(1)), v128_or(cmp!(2), cmp!(3))),
      v128_or(v128_or(cmp!(4), cmp!(5)), cmp!(6)),
    ),
    8 => v128_or(
      v128_or(v128_or(cmp!(0), cmp!(1)), v128_or(cmp!(2), cmp!(3))),
      v128_or(v128_or(cmp!(4), cmp!(5)), v128_or(cmp!(6), cmp!(7))),
    ),
    _ => eq_any_mask_dynamic_simd128(chunk, &needles),
  }
}
