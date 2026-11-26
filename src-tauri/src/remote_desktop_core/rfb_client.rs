use super::{
    rfb_framebuffer::RfbFramebufferTransaction, ClientFramebufferCore, ClientFramebufferHub,
    FrameId, FramebufferGeometry, FramebufferRect,
};
use flate2::{Decompress, FlushDecompress};
use serde::Serialize;
use std::ops::Range;

#[cfg(test)]
#[path = "../../tests/support/rfb_decoder_bench.rs"]
mod benchmarks;

const FRAMEBUFFER_UPDATE: u8 = 0;
const RAW_ENCODING: i32 = 0;
const COPY_RECT_ENCODING: i32 = 1;
const HEXTILE_ENCODING: i32 = 5;
const TIGHT_ENCODING: i32 = 7;
const TIGHT_PNG_ENCODING: i32 = -260;
const DESKTOP_SIZE_ENCODING: i32 = -223;
const LAST_RECT_ENCODING: i32 = -224;
const POINTER_POS_ENCODING: i32 = -232;
const RICH_CURSOR_ENCODING: i32 = -239;
const X_CURSOR_ENCODING: i32 = -240;
const EXTENDED_DESKTOP_SIZE_ENCODING: i32 = -308;
const CONTINUOUS_UPDATES_ENCODING: i32 = -313;
const ZLIB_ENCODING: i32 = 6;
const ZRLE_ENCODING: i32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfbFramebufferUpdateOutcome {
    pub encodings: super::RfbEncodingCounts,
    pub frame_id: Option<FrameId>,
    pub geometry_changed: bool,
    /// Legacy DesktopSize discards pixels; ExtendedDesktopSize must retain them
    /// and must not cause a non-incremental request loop.
    pub full_refresh_required: bool,
    pub cursor: Option<RfbCursorUpdate>,
    pub pointer_position: Option<(u32, u32)>,
    pub desktop_layout: Option<Vec<RfbDesktopScreen>>,
    pub continuous_updates_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfbCursorUpdate {
    pub hotspot_x: u32,
    pub hotspot_y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RfbDesktopScreen {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub flags: u32,
}

/// The server's negotiated RFB pixel layout. Decoded rectangles are normalized to
/// the Core's BGRA8888 layout before any Surface can observe them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientRfbPixelFormat {
    pub bits_per_pixel: u8,
    pub depth: u8,
    pub big_endian: bool,
    pub true_color: bool,
    pub red_max: u16,
    pub green_max: u16,
    pub blue_max: u16,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

impl ClientRfbPixelFormat {
    /// Canonical 32-bit little-endian true-colour request used by the native Core:
    /// bytes arrive as B, G, R, padding and map directly to BGRA storage.
    pub const fn core_bgra32() -> Self {
        Self {
            bits_per_pixel: 32,
            depth: 24,
            big_endian: false,
            true_color: true,
            red_max: 255,
            green_max: 255,
            blue_max: 255,
            red_shift: 16,
            green_shift: 8,
            blue_shift: 0,
        }
    }

    fn validate(self) -> Result<(), String> {
        if self.bits_per_pixel != 32 || !self.true_color {
            return Err("rfb_unsupported_pixel_format".into());
        }
        if self.red_max == 0 || self.green_max == 0 || self.blue_max == 0 {
            return Err("rfb_invalid_pixel_format".into());
        }
        self.cpixel_range()?;
        Ok(())
    }

    pub(super) fn tight_pixel_bytes(self) -> usize {
        if self.true_color
            && self.bits_per_pixel == 32
            && self.depth == 24
            && self.red_max == 255
            && self.green_max == 255
            && self.blue_max == 255
        {
            3
        } else {
            usize::from(self.bits_per_pixel / 8)
        }
    }

    fn cpixel_range(self) -> Result<Range<usize>, String> {
        super::cpixel_byte_range(
            self.bits_per_pixel,
            self.depth,
            self.big_endian,
            [
                (self.red_max, self.red_shift),
                (self.green_max, self.green_shift),
                (self.blue_max, self.blue_shift),
            ],
        )
        .map_err(str::to_string)
    }
}

/// RFB client-side message boundary for framebuffer data. It deliberately accepts
/// one complete server message: incremental TCP framing stays in
/// `RfbClientSessionCore`, while decoding and canonical framebuffer ownership remain
/// independent of sockets.
pub struct RfbClientFramebufferDecoder {
    pixel_format: ClientRfbPixelFormat,
    cpixel_range: Range<usize>,
    zlib_decoder: Decompress,
    zrle_decoder: Decompress,
    tight_decoders: [Decompress; 4],
    transaction: RfbFramebufferTransaction,
    memory: super::ClientMemoryBudget,
}

impl RfbClientFramebufferDecoder {
    pub fn new(pixel_format: ClientRfbPixelFormat) -> Result<Self, String> {
        Self::with_memory(pixel_format, super::ClientMemoryBudget::default())
    }

    pub(super) fn with_memory(
        pixel_format: ClientRfbPixelFormat,
        memory: super::ClientMemoryBudget,
    ) -> Result<Self, String> {
        pixel_format.validate()?;
        Ok(Self {
            pixel_format,
            cpixel_range: pixel_format.cpixel_range()?,
            zlib_decoder: Decompress::new(true),
            zrle_decoder: Decompress::new(true),
            tight_decoders: std::array::from_fn(|_| Decompress::new(true)),
            transaction: RfbFramebufferTransaction::with_memory(&memory),
            memory,
        })
    }

    pub fn decode_framebuffer_update(
        &mut self,
        message: &[u8],
        geometry: FramebufferGeometry,
        framebuffer: &mut ClientFramebufferCore,
    ) -> Result<RfbFramebufferUpdateOutcome, String> {
        self.bind_memory(framebuffer.memory_budget());
        self.transaction.reserve(geometry)?;
        self.transaction.prepare(framebuffer, geometry);
        let mut outcome = self.decode_into_transaction(message, geometry)?;
        outcome.frame_id = self.transaction.commit(framebuffer)?;
        Ok(outcome)
    }

    /// Runtime entry: hold the Surface publication lock only for baseline repair
    /// and the final swap; rectangle decoding works on a private back buffer.
    pub(super) fn decode_to_hub(
        &mut self,
        message: &[u8],
        geometry: FramebufferGeometry,
        hub: &ClientFramebufferHub,
    ) -> Result<RfbFramebufferUpdateOutcome, String> {
        self.bind_memory(hub.memory_budget());
        self.transaction.reserve(geometry)?;
        self.transaction.prepare(&*hub.lock()?, geometry);
        let mut outcome = self.decode_into_transaction(message, geometry)?;
        outcome.frame_id = self.transaction.commit(&mut *hub.lock()?)?;
        Ok(outcome)
    }

    fn bind_memory(&mut self, memory: super::ClientMemoryBudget) {
        if !self.memory.same_owner(&memory) {
            self.transaction = RfbFramebufferTransaction::with_memory(&memory);
            self.memory = memory;
        }
    }

    fn decode_into_transaction(
        &mut self,
        message: &[u8],
        geometry: FramebufferGeometry,
    ) -> Result<RfbFramebufferUpdateOutcome, String> {
        if message.len() > super::rfb_limits::MAX_SERVER_MESSAGE_BYTES {
            return Err("rfb_resource_limit:server_message".into());
        }
        let mut reader = MessageReader::new(message);
        if reader.read_u8()? != FRAMEBUFFER_UPDATE {
            return Err("rfb_expected_framebuffer_update".into());
        }
        reader.skip(1)?; // padding
        let rectangle_count = reader.read_u16()?;
        let mut effective_geometry = geometry;
        let mut geometry_changed = false;
        let mut full_refresh_required = false;
        let mut cursor = None;
        let mut pointer_position = None;
        let mut desktop_layout = None;
        let mut continuous_updates_supported = false;
        let mut decoded_bytes = 0usize;
        let mut encodings = super::RfbEncodingCounts::default();
        for _ in 0..rectangle_count {
            let rect = FramebufferRect {
                x: reader.read_u16()? as u32,
                y: reader.read_u16()? as u32,
                width: reader.read_u16()? as u32,
                height: reader.read_u16()? as u32,
            };
            let encoding = reader.read_i32()?;
            if encoding == LAST_RECT_ENCODING {
                break;
            }
            if encoding == DESKTOP_SIZE_ENCODING {
                if rect.x != 0 || rect.y != 0 || rect.width == 0 || rect.height == 0 {
                    return Err("rfb_invalid_desktop_size".into());
                }
                effective_geometry.width = rect.width;
                effective_geometry.height = rect.height;
                geometry_changed = true;
                full_refresh_required = true;
                self.transaction.resize(effective_geometry)?;
                continue;
            }
            if encoding == EXTENDED_DESKTOP_SIZE_ENCODING {
                let screen_count = usize::from(reader.read_u8()?);
                reader.skip(3)?;
                if rect.x == 1 && rect.y != 0 {
                    // Includes "request forwarded" and unknown failure codes:
                    // geometry and SCREEN fields are undefined but framing is mandatory.
                    reader.skip(screen_count * 16)?;
                    continue;
                }
                if rect.width == 0 || rect.height == 0 {
                    return Err("rfb_invalid_extended_desktop_size".into());
                }
                let mut screens = Vec::with_capacity(screen_count);
                for _ in 0..screen_count {
                    screens.push(RfbDesktopScreen {
                        id: reader.read_u32()?,
                        x: i32::from(reader.read_u16()?),
                        y: i32::from(reader.read_u16()?),
                        width: u32::from(reader.read_u16()?),
                        height: u32::from(reader.read_u16()?),
                        flags: reader.read_u32()?,
                    });
                }
                if effective_geometry.width != rect.width
                    || effective_geometry.height != rect.height
                {
                    effective_geometry.width = rect.width;
                    effective_geometry.height = rect.height;
                    geometry_changed = true;
                    self.transaction
                        .resize_preserving_pixels(effective_geometry)?;
                }
                desktop_layout = Some(screens);
                continue;
            }
            if encoding == POINTER_POS_ENCODING {
                pointer_position = Some((rect.x, rect.y));
                continue;
            }
            if encoding == CONTINUOUS_UPDATES_ENCODING {
                continuous_updates_supported = true;
                continue;
            }
            if encoding == RICH_CURSOR_ENCODING {
                let bytes =
                    super::rfb_limits::cursor_bytes(rect.width as usize, rect.height as usize)?;
                decoded_bytes = decoded_bytes
                    .checked_add(bytes)
                    .filter(|bytes| *bytes <= super::rfb_limits::MAX_FRAMEBUFFER_BYTES)
                    .ok_or("rfb_resource_limit:decoded_update")?;
                let mask_bytes = (rect.width as usize)
                    .div_ceil(8)
                    .checked_mul(rect.height as usize)
                    .ok_or("rfb_cursor_size_overflow")?;
                let raw = reader.read_exact(rect_byte_len(rect)?)?;
                let mut pixels = self.decode_raw_pixels(raw)?;
                let mask = reader.read_exact(mask_bytes)?;
                apply_cursor_mask(&mut pixels, mask, rect.width as usize, rect.height as usize)?;
                cursor = Some(RfbCursorUpdate {
                    hotspot_x: rect.x,
                    hotspot_y: rect.y,
                    width: rect.width,
                    height: rect.height,
                    pixels,
                });
                continue;
            }
            if encoding == X_CURSOR_ENCODING {
                let bytes =
                    super::rfb_limits::cursor_bytes(rect.width as usize, rect.height as usize)?;
                decoded_bytes = decoded_bytes
                    .checked_add(bytes)
                    .filter(|bytes| *bytes <= super::rfb_limits::MAX_FRAMEBUFFER_BYTES)
                    .ok_or("rfb_resource_limit:decoded_update")?;
                // Empty XCursor rectangles hide the cursor and have no color or bitmap body.
                if bytes == 0 {
                    cursor = Some(RfbCursorUpdate {
                        hotspot_x: rect.x,
                        hotspot_y: rect.y,
                        width: rect.width,
                        height: rect.height,
                        pixels: Vec::new(),
                    });
                    continue;
                }
                let bitmap_bytes = (rect.width as usize)
                    .div_ceil(8)
                    .checked_mul(rect.height as usize)
                    .ok_or("rfb_cursor_size_overflow")?;
                let colors = reader.read_exact(6)?;
                let bitmap = reader.read_exact(bitmap_bytes)?;
                let mask = reader.read_exact(bitmap_bytes)?;
                cursor = Some(RfbCursorUpdate {
                    hotspot_x: rect.x,
                    hotspot_y: rect.y,
                    width: rect.width,
                    height: rect.height,
                    pixels: decode_x_cursor(
                        colors,
                        bitmap,
                        mask,
                        rect.width as usize,
                        rect.height as usize,
                    )?,
                });
                continue;
            }
            if !rect.is_within(effective_geometry) {
                return Err("rfb_framebuffer_rect_out_of_bounds".into());
            }
            let rect_bytes = rect_byte_len(rect)?;
            decoded_bytes = decoded_bytes
                .checked_add(rect_bytes)
                .filter(|bytes| *bytes <= super::rfb_limits::MAX_FRAMEBUFFER_BYTES)
                .ok_or("rfb_resource_limit:decoded_update")?;
            encodings.rectangles += 1;
            if encoding == RAW_ENCODING {
                let wire = reader.read_exact(rect_bytes)?;
                let format = self.pixel_format;
                self.transaction.apply_pixels(
                    rect,
                    wire.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|pixel| decode_wire_pixel(format, *pixel)),
                )?;
                continue;
            }
            if encoding == COPY_RECT_ENCODING {
                let source_x = u32::from(reader.read_u16()?);
                let source_y = u32::from(reader.read_u16()?);
                self.transaction.copy_rect(source_x, source_y, rect)?;
                encodings.copy_rect += 1;
                continue;
            }
            let scratch_bytes = match encoding {
                HEXTILE_ENCODING => rect_bytes,
                ZLIB_ENCODING => rect_bytes
                    .checked_add(1)
                    .ok_or("rfb_resource_limit:session_memory")?,
                ZRLE_ENCODING => zrle_decoded_limit(rect, self.cpixel_range.len())?
                    .checked_add(rect_bytes)
                    .ok_or("rfb_resource_limit:session_memory")?,
                TIGHT_ENCODING | TIGHT_PNG_ENCODING
                    if reader.remaining.first().is_some_and(|control| {
                        matches!(
                            super::rfb_tight::TightMode::parse(*control, encoding),
                            Ok(super::rfb_tight::TightMode::Fill)
                        )
                    }) =>
                {
                    0
                }
                TIGHT_ENCODING | TIGHT_PNG_ENCODING
                    if reader.remaining.first().is_some_and(|control| {
                        super::rfb_tight::TightMode::parse(*control, encoding)
                            .is_ok_and(|mode| mode.is_image())
                    }) =>
                {
                    2 * super::rfb_limits::MAX_FRAMEBUFFER_BYTES + rect_bytes
                }
                TIGHT_ENCODING | TIGHT_PNG_ENCODING => rect_bytes
                    .checked_mul(5)
                    .and_then(|bytes| bytes.checked_div(2))
                    .ok_or("rfb_resource_limit:session_memory")?,
                _ => 0,
            };
            let _scratch = self.memory.reserve(scratch_bytes)?;
            if matches!(encoding, TIGHT_ENCODING | TIGHT_PNG_ENCODING) {
                encodings.tight += 1;
                if reader
                    .remaining
                    .first()
                    .is_some_and(|control| control >> 4 == 9)
                {
                    encodings.jpeg += 1;
                }
            }
            let mut pixels = match encoding {
                HEXTILE_ENCODING => self.decode_hextile(&mut reader, rect)?,
                TIGHT_ENCODING | TIGHT_PNG_ENCODING => {
                    match self.decode_tight(&mut reader, rect, encoding)? {
                        TightDecodedPixels::Fill(pixel) => {
                            self.transaction
                                .apply_pixels(rect, std::iter::repeat_n(pixel, rect_bytes / 4))?;
                            continue;
                        }
                        TightDecodedPixels::Pixels(pixels) => pixels,
                    }
                }
                ZLIB_ENCODING => {
                    let compressed_len = reader.read_u32()? as usize;
                    let compressed = reader.read_exact(compressed_len)?;
                    self.decompress_zlib(compressed, rect_byte_len(rect)?)?
                }
                ZRLE_ENCODING => {
                    let compressed_len = reader.read_u32()? as usize;
                    let compressed = reader.read_exact(compressed_len)?;
                    self.decode_zrle_tiles(compressed, rect)?
                }
                _ => return Err(format!("rfb_unsupported_framebuffer_encoding:{encoding}")),
            };
            if encoding == ZLIB_ENCODING {
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    let decoded = decode_wire_pixel(self.pixel_format, *pixel);
                    pixel.copy_from_slice(&decoded);
                }
            }
            self.transaction.apply(rect, &pixels)?;
        }
        if !reader.is_finished() {
            return Err("rfb_framebuffer_update_trailing_bytes".into());
        }
        Ok(RfbFramebufferUpdateOutcome {
            encodings,
            frame_id: None,
            geometry_changed,
            full_refresh_required,
            cursor,
            pointer_position,
            desktop_layout,
            continuous_updates_supported,
        })
    }

    fn decompress_zlib(&mut self, compressed: &[u8], expected: usize) -> Result<Vec<u8>, String> {
        let before_in = self.zlib_decoder.total_in();
        // Input can be fully consumed while inflate still holds excess output.
        // Keep one guard byte so an exact-size rectangle cannot hide that output.
        let mut decoded = Vec::with_capacity(expected + 1);
        self.zlib_decoder
            .decompress_vec(compressed, &mut decoded, FlushDecompress::Sync)
            .map_err(|_| "rfb_zlib_decompression_failed")?;
        if self.zlib_decoder.total_in() - before_in != compressed.len() as u64
            || decoded.len() != expected
        {
            return Err("rfb_zlib_decoded_length_mismatch".into());
        }
        Ok(decoded)
    }

    fn decode_zrle_tiles(
        &mut self,
        compressed: &[u8],
        rect: FramebufferRect,
    ) -> Result<Vec<u8>, String> {
        let capacity = zrle_decoded_limit(rect, self.cpixel_range.len())?;
        let before_in = self.zrle_decoder.total_in();
        let mut decoded = Vec::with_capacity(capacity);
        self.zrle_decoder
            .decompress_vec(compressed, &mut decoded, FlushDecompress::Sync)
            .map_err(|_| "rfb_zrle_decompression_failed")?;
        if self.zrle_decoder.total_in() - before_in != compressed.len() as u64 {
            return Err("rfb_zrle_compressed_length_mismatch".into());
        }
        let mut reader = MessageReader::new(&decoded);
        let mut pixels = vec![0; rect_byte_len(rect)?];
        for tile_y in (0..rect.height).step_by(64) {
            for tile_x in (0..rect.width).step_by(64) {
                let tile_height = (rect.height - tile_y).min(64);
                let tile_width = (rect.width - tile_x).min(64);
                let subencoding = reader.read_u8()?;
                if !matches!(subencoding,0..=16|128|130..=255) {
                    return Err("rfb_zrle_invalid_subencoding".into());
                }
                let palette_size = usize::from(subencoding & 0x7f);
                let palette = if palette_size == 0 {
                    Vec::new()
                } else {
                    (0..palette_size)
                        .map(|_| {
                            read_zrle_pixel(&mut reader, self.pixel_format, &self.cpixel_range)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                };
                let tile_pixels = usize::try_from(tile_width)
                    .ok()
                    .and_then(|width| {
                        usize::try_from(tile_height)
                            .ok()
                            .and_then(|height| width.checked_mul(height))
                    })
                    .ok_or_else(|| "rfb_zrle_tile_size_overflow".to_string())?;
                let decoded_tile = match subencoding {
                    0 => (0..tile_pixels)
                        .map(|_| {
                            read_zrle_pixel(&mut reader, self.pixel_format, &self.cpixel_range)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    1 => vec![palette[0]; tile_pixels],
                    2..=16 => decode_zrle_packed_palette(
                        &mut reader,
                        &palette,
                        tile_width as usize,
                        tile_height as usize,
                    )?,
                    128 => decode_zrle_plain_rle(
                        &mut reader,
                        tile_pixels,
                        self.pixel_format,
                        &self.cpixel_range,
                    )?,
                    130..=255 => decode_zrle_palette_rle(&mut reader, &palette, tile_pixels)?,
                    _ => unreachable!(),
                };
                write_zrle_tile(
                    &mut pixels,
                    rect.width as usize,
                    tile_x as usize,
                    tile_y as usize,
                    tile_width as usize,
                    tile_height as usize,
                    &decoded_tile,
                )?;
            }
        }
        if !reader.is_finished() {
            return Err("rfb_zrle_decoded_trailing_bytes".into());
        }
        Ok(pixels)
    }

    fn decode_raw_pixels(&self, wire_pixels: &[u8]) -> Result<Vec<u8>, String> {
        let mut decoded = Vec::with_capacity(wire_pixels.len());
        for pixel in wire_pixels.as_chunks::<4>().0 {
            decoded.extend_from_slice(&decode_wire_pixel(self.pixel_format, *pixel));
        }
        Ok(decoded)
    }

    fn decode_hextile(
        &self,
        reader: &mut MessageReader<'_>,
        rect: FramebufferRect,
    ) -> Result<Vec<u8>, String> {
        let mut output = vec![0; rect_byte_len(rect)?];
        let mut background = [0, 0, 0, 255];
        let mut foreground = [0, 0, 0, 255];
        let mut colors = super::rfb_hextile::HextileColorState::default();
        for tile_y in (0..rect.height as usize).step_by(16) {
            for tile_x in (0..rect.width as usize).step_by(16) {
                let width = (rect.width as usize - tile_x).min(16);
                let height = (rect.height as usize - tile_y).min(16);
                let flags = reader.read_u8()?;
                colors.begin_tile(flags)?;
                if flags & 1 != 0 {
                    let raw = reader.read_exact(width * height * 4)?;
                    let decoded = self.decode_raw_pixels(raw)?;
                    write_bgra_tile(
                        &mut output,
                        rect.width as usize,
                        tile_x,
                        tile_y,
                        width,
                        height,
                        &decoded,
                    )?;
                    continue;
                }
                if flags & 2 != 0 {
                    background = self.read_wire_pixel(reader)?;
                }
                fill_bgra_rect(
                    &mut output,
                    rect.width as usize,
                    tile_x,
                    tile_y,
                    width,
                    height,
                    background,
                )?;
                if flags & 4 != 0 {
                    foreground = self.read_wire_pixel(reader)?;
                }
                if flags & 8 == 0 {
                    continue;
                }
                let count = reader.read_u8()?;
                colors.check_subrectangles(flags, count)?;
                for _ in 0..count {
                    let color = if flags & 16 != 0 {
                        self.read_wire_pixel(reader)?
                    } else {
                        foreground
                    };
                    let xy = reader.read_u8()?;
                    let wh = reader.read_u8()?;
                    let x = tile_x + usize::from(xy >> 4);
                    let y = tile_y + usize::from(xy & 0x0f);
                    let sub_width = usize::from(wh >> 4) + 1;
                    let sub_height = usize::from(wh & 0x0f) + 1;
                    if x + sub_width > tile_x + width || y + sub_height > tile_y + height {
                        return Err("rfb_hextile_subrect_out_of_bounds".into());
                    }
                    fill_bgra_rect(
                        &mut output,
                        rect.width as usize,
                        x,
                        y,
                        sub_width,
                        sub_height,
                        color,
                    )?;
                }
            }
        }
        Ok(output)
    }

    fn read_wire_pixel(&self, reader: &mut MessageReader<'_>) -> Result<[u8; 4], String> {
        let decoded = self.decode_raw_pixels(reader.read_exact(4)?)?;
        Ok(decoded[..4].try_into().expect("one decoded pixel"))
    }
}

include!("rfb_client_tight.rs");

fn apply_cursor_mask(
    pixels: &mut [u8],
    mask: &[u8],
    width: usize,
    height: usize,
) -> Result<(), String> {
    let row_bytes = width.div_ceil(8);
    if pixels.len() != width * height * 4 || mask.len() != row_bytes * height {
        return Err("rfb_cursor_length_mismatch".into());
    }
    for y in 0..height {
        for x in 0..width {
            let visible = (mask[y * row_bytes + x / 8] >> (7 - x % 8)) & 1 != 0;
            pixels[(y * width + x) * 4 + 3] = if visible { 255 } else { 0 };
        }
    }
    Ok(())
}

fn decode_x_cursor(
    colors: &[u8],
    bitmap: &[u8],
    mask: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<u8>, String> {
    let row_bytes = width.div_ceil(8);
    if colors.len() != 6 || bitmap.len() != row_bytes * height || mask.len() != bitmap.len() {
        return Err("rfb_cursor_length_mismatch".into());
    }
    let foreground = [colors[2], colors[1], colors[0], 255];
    let background = [colors[5], colors[4], colors[3], 255];
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let bit = 1 << (7 - x % 8);
            let mut pixel = if bitmap[y * row_bytes + x / 8] & bit != 0 {
                foreground
            } else {
                background
            };
            if mask[y * row_bytes + x / 8] & bit == 0 {
                pixel[3] = 0;
            }
            pixels.extend_from_slice(&pixel);
        }
    }
    Ok(pixels)
}

fn fill_bgra_rect(
    output: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) -> Result<(), String> {
    for row in y..y + height {
        let start = (row * stride + x)
            .checked_mul(4)
            .ok_or("rfb_hextile_size_overflow")?;
        let end = start + width * 4;
        let target = output
            .get_mut(start..end)
            .ok_or("rfb_hextile_subrect_out_of_bounds")?;
        for pixel in target.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&color);
        }
    }
    Ok(())
}

fn write_bgra_tile(
    output: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    tile: &[u8],
) -> Result<(), String> {
    if tile.len() != width * height * 4 {
        return Err("rfb_hextile_tile_length_mismatch".into());
    }
    for row in 0..height {
        let source = &tile[row * width * 4..(row + 1) * width * 4];
        let start = ((y + row) * stride + x) * 4;
        output
            .get_mut(start..start + source.len())
            .ok_or("rfb_hextile_tile_out_of_bounds")?
            .copy_from_slice(source);
    }
    Ok(())
}

fn rect_byte_len(rect: FramebufferRect) -> Result<usize, String> {
    usize::try_from(rect.width)
        .map_err(|_| "rfb_framebuffer_rectangle_size_overflow".to_string())?
        .checked_mul(
            usize::try_from(rect.height).map_err(|_| "rfb_framebuffer_rectangle_size_overflow")?,
        )
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "rfb_framebuffer_rectangle_size_overflow".to_string())
}

fn zrle_decoded_limit(rect: FramebufferRect, cpixel_bytes: usize) -> Result<usize, String> {
    let pixels = rect_byte_len(rect)?
        .checked_div(4)
        .ok_or_else(|| "rfb_zrle_rectangle_size_overflow".to_string())?;
    let tiles = (rect.width.div_ceil(64) as usize)
        .checked_mul(rect.height.div_ceil(64) as usize)
        .ok_or_else(|| "rfb_zrle_rectangle_size_overflow".to_string())?;
    pixels
        .checked_mul(cpixel_bytes + 1)
        .and_then(|bytes| {
            tiles
                .checked_mul(512)
                .and_then(|tile_bytes| bytes.checked_add(tile_bytes))
        })
        .ok_or_else(|| "rfb_zrle_rectangle_size_overflow".to_string())
}

fn read_zrle_pixel(
    reader: &mut MessageReader<'_>,
    format: ClientRfbPixelFormat,
    range: &Range<usize>,
) -> Result<[u8; 4], String> {
    let mut pixel = [0; 4];
    pixel[range.clone()].copy_from_slice(reader.read_exact(range.len())?);
    Ok(decode_wire_pixel(format, pixel))
}

fn decode_wire_pixel(format: ClientRfbPixelFormat, pixel: [u8; 4]) -> [u8; 4] {
    let value = if format.big_endian {
        u32::from_be_bytes(pixel)
    } else {
        u32::from_le_bytes(pixel)
    };
    [
        scale_component(value, format.blue_shift, format.blue_max),
        scale_component(value, format.green_shift, format.green_max),
        scale_component(value, format.red_shift, format.red_max),
        255,
    ]
}

fn decode_zrle_packed_palette(
    reader: &mut MessageReader<'_>,
    palette: &[[u8; 4]],
    width: usize,
    height: usize,
) -> Result<Vec<[u8; 4]>, String> {
    let bits = match palette.len() {
        1..=2 => 1,
        3..=4 => 2,
        5..=16 => 4,
        _ => return Err("rfb_zrle_invalid_packed_palette".into()),
    };
    let row_bytes = width
        .checked_mul(bits)
        .and_then(|bits| bits.checked_add(7))
        .map(|bits| bits / 8)
        .ok_or_else(|| "rfb_zrle_tile_size_overflow".to_string())?;
    let mut output = Vec::with_capacity(width * height);
    for _ in 0..height {
        let row = reader.read_exact(row_bytes)?;
        for x in 0..width {
            let bit_offset = x * bits;
            let shift = 8 - bits - (bit_offset % 8);
            let index = usize::from((row[bit_offset / 8] >> shift) & ((1 << bits) - 1));
            output.push(
                *palette
                    .get(index)
                    .ok_or("rfb_zrle_palette_index_out_of_bounds")?,
            );
        }
    }
    Ok(output)
}

fn decode_zrle_plain_rle(
    reader: &mut MessageReader<'_>,
    pixel_count: usize,
    format: ClientRfbPixelFormat,
    range: &Range<usize>,
) -> Result<Vec<[u8; 4]>, String> {
    let mut output = Vec::with_capacity(pixel_count);
    while output.len() < pixel_count {
        let pixel = read_zrle_pixel(reader, format, range)?;
        let run = read_zrle_run_length(reader)?;
        if output
            .len()
            .checked_add(run)
            .is_none_or(|length| length > pixel_count)
        {
            return Err("rfb_zrle_run_exceeds_tile".into());
        }
        output.extend(std::iter::repeat_n(pixel, run));
    }
    Ok(output)
}

fn decode_zrle_palette_rle(
    reader: &mut MessageReader<'_>,
    palette: &[[u8; 4]],
    pixel_count: usize,
) -> Result<Vec<[u8; 4]>, String> {
    let mut output = Vec::with_capacity(pixel_count);
    while output.len() < pixel_count {
        let index = reader.read_u8()?;
        let pixel = *palette
            .get(usize::from(index & 0x7f))
            .ok_or("rfb_zrle_palette_index_out_of_bounds")?;
        let run = if index & 0x80 == 0 {
            1
        } else {
            read_zrle_run_length(reader)?
        };
        if output
            .len()
            .checked_add(run)
            .is_none_or(|length| length > pixel_count)
        {
            return Err("rfb_zrle_run_exceeds_tile".into());
        }
        output.extend(std::iter::repeat_n(pixel, run));
    }
    Ok(output)
}

fn read_zrle_run_length(reader: &mut MessageReader<'_>) -> Result<usize, String> {
    let mut length = 1usize;
    loop {
        let byte = usize::from(reader.read_u8()?);
        length = length
            .checked_add(byte)
            .ok_or_else(|| "rfb_zrle_run_length_overflow".to_string())?;
        if byte != 255 {
            return Ok(length);
        }
    }
}

fn write_zrle_tile(
    output: &mut [u8],
    rect_width: usize,
    tile_x: usize,
    tile_y: usize,
    tile_width: usize,
    tile_height: usize,
    tile: &[[u8; 4]],
) -> Result<(), String> {
    if tile.len() != tile_width * tile_height {
        return Err("rfb_zrle_tile_length_mismatch".into());
    }
    for row in 0..tile_height {
        let source_start = row * tile_width;
        let target_start = ((tile_y + row) * rect_width + tile_x) * 4;
        let target_end = target_start + tile_width * 4;
        for (target, pixel) in output[target_start..target_end]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&tile[source_start..source_start + tile_width])
        {
            target.copy_from_slice(pixel);
        }
    }
    Ok(())
}

fn scale_component(value: u32, shift: u8, maximum: u16) -> u8 {
    let component = (value >> shift) & u32::from(maximum);
    if maximum == 255 {
        // An 8-bit channel is already canonical; avoid three divisions per pixel.
        component as u8
    } else {
        ((component * 255) / u32::from(maximum)) as u8
    }
}

struct MessageReader<'a> {
    remaining: &'a [u8],
}

impl<'a> MessageReader<'a> {
    fn new(message: &'a [u8]) -> Self {
        Self { remaining: message }
    }

    fn is_finished(&self) -> bool {
        self.remaining.is_empty()
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(
            self.read_exact(2)?.try_into().expect("exact u16 width"),
        ))
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(
            self.read_exact(4)?.try_into().expect("exact i32 width"),
        ))
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(
            self.read_exact(4)?.try_into().expect("exact u32 width"),
        ))
    }

    fn read_compact_length(&mut self) -> Result<usize, String> {
        let first = self.read_u8()?;
        let mut length = usize::from(first & 0x7f);
        if first & 0x80 == 0 {
            return Ok(length);
        }
        let second = self.read_u8()?;
        length |= usize::from(second & 0x7f) << 7;
        if second & 0x80 == 0 {
            return Ok(length);
        }
        length |= usize::from(self.read_u8()?) << 14;
        Ok(length)
    }

    fn skip(&mut self, length: usize) -> Result<(), String> {
        self.read_exact(length).map(|_| ())
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], String> {
        if self.remaining.len() < length {
            return Err("rfb_framebuffer_update_truncated".into());
        }
        let (read, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(read)
    }
}
