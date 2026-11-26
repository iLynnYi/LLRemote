use std::ops::Range;

/// RFC 6143 CPIXEL is the negotiated PIXEL with one unused edge byte omitted,
/// when all colour bits fit that way. The range indexes bytes in wire order.
pub(crate) fn cpixel_byte_range(
    bits: u8,
    depth: u8,
    big_endian: bool,
    channels: [(u16, u8); 3],
) -> Result<Range<usize>, &'static str> {
    if !matches!(bits, 8 | 16 | 32) || depth == 0 || depth > bits {
        return Err("rfb_invalid_pixel_format");
    }
    let mut used = 0u64;
    for (max, shift) in channels {
        if !(u32::from(max) + 1).is_power_of_two() || shift >= bits {
            return Err("rfb_invalid_pixel_format");
        }
        let mask = u64::from(max) << shift;
        if mask >> bits != 0 || used & mask != 0 {
            return Err("rfb_invalid_pixel_format");
        }
        used |= mask;
    }
    if used.count_ones() > u32::from(depth) {
        return Err("rfb_invalid_pixel_format");
    }
    if bits == 32 && depth <= 24 {
        if used & 0xff00_0000 == 0 {
            return Ok(if big_endian { 1..4 } else { 0..3 });
        }
        if used & 0x0000_00ff == 0 {
            return Ok(if big_endian { 0..3 } else { 1..4 });
        }
    }
    Ok(0..usize::from(bits / 8))
}
