use core::arch::aarch64::*;

#[inline(always)]
pub(in crate::needles) fn eq_any_mask_dynamic(chunk: uint8x16_t, needles: &[u8]) -> uint8x16_t {
  let mut any_mask = unsafe { vdupq_n_u8(0) };

  for &needle in needles {
    let target = unsafe { vdupq_n_u8(needle) };
    let cmp = unsafe { vceqq_u8(chunk, target) };
    any_mask = unsafe { vorrq_u8(any_mask, cmp) };
  }

  any_mask
}

#[inline(always)]
pub(in crate::needles) fn eq_any_mask_const<const N: usize>(
  chunk: uint8x16_t,
  needles: [u8; N],
) -> uint8x16_t {
  match N {
    0 => unsafe { vdupq_n_u8(0) },
    1 => {
      let target = unsafe { vdupq_n_u8(needles[0]) };
      unsafe { vceqq_u8(chunk, target) }
    }
    2 => {
      let c0 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[0])) };
      let c1 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[1])) };
      unsafe { vorrq_u8(c0, c1) }
    }
    3 => {
      let c0 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[0])) };
      let c1 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[1])) };
      let c2 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[2])) };
      unsafe { vorrq_u8(vorrq_u8(c0, c1), c2) }
    }
    4 => {
      let c0 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[0])) };
      let c1 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[1])) };
      let c2 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[2])) };
      let c3 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[3])) };
      unsafe { vorrq_u8(vorrq_u8(c0, c1), vorrq_u8(c2, c3)) }
    }
    5 => {
      let c0 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[0])) };
      let c1 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[1])) };
      let c2 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[2])) };
      let c3 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[3])) };
      let c4 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[4])) };
      unsafe { vorrq_u8(vorrq_u8(vorrq_u8(c0, c1), vorrq_u8(c2, c3)), c4) }
    }
    6 => {
      let c0 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[0])) };
      let c1 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[1])) };
      let c2 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[2])) };
      let c3 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[3])) };
      let c4 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[4])) };
      let c5 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[5])) };
      unsafe {
        vorrq_u8(
          vorrq_u8(vorrq_u8(c0, c1), vorrq_u8(c2, c3)),
          vorrq_u8(c4, c5),
        )
      }
    }
    7 => {
      let c0 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[0])) };
      let c1 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[1])) };
      let c2 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[2])) };
      let c3 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[3])) };
      let c4 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[4])) };
      let c5 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[5])) };
      let c6 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[6])) };
      unsafe {
        vorrq_u8(
          vorrq_u8(vorrq_u8(c0, c1), vorrq_u8(c2, c3)),
          vorrq_u8(vorrq_u8(c4, c5), c6),
        )
      }
    }
    8 => {
      let c0 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[0])) };
      let c1 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[1])) };
      let c2 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[2])) };
      let c3 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[3])) };
      let c4 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[4])) };
      let c5 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[5])) };
      let c6 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[6])) };
      let c7 = unsafe { vceqq_u8(chunk, vdupq_n_u8(needles[7])) };
      unsafe {
        vorrq_u8(
          vorrq_u8(vorrq_u8(c0, c1), vorrq_u8(c2, c3)),
          vorrq_u8(vorrq_u8(c4, c5), vorrq_u8(c6, c7)),
        )
      }
    }
    _ => eq_any_mask_dynamic(chunk, &needles),
  }
}

/// Returns a `uint8x16_t` where each byte lane is `0xFF` if the corresponding
/// byte in `chunk` is not ASCII (`0x80..=0xFF`), or `0x00` otherwise.
///
/// The non-ASCII bytes are exactly the lanes that are negative as `i8`, so one
/// compare against zero (`cmlt #0`) is the whole test.
#[inline(always)]
pub(in crate::needles) fn high_bit_mask(chunk: uint8x16_t) -> uint8x16_t {
  unsafe { vcltzq_s8(vreinterpretq_s8_u8(chunk)) }
}
