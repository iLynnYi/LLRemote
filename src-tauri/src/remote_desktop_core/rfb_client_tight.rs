use super::rfb_tight::TightMode;

// Fill carries one canonical pixel; expanding it is the transaction's job.
enum TightDecodedPixels {
    Fill([u8; 4]),
    Pixels(Vec<u8>),
}

impl RfbClientFramebufferDecoder {
    fn decode_tight(
        &mut self,
        reader: &mut MessageReader<'_>,
        rect: FramebufferRect,
        encoding: i32,
    ) -> Result<TightDecodedPixels, String> {
        let control = reader.read_u8()?;
        for stream in 0..4 {
            if control & (1 << stream) != 0 {
                self.tight_decoders[stream] = Decompress::new(true);
            }
        }
        let mode = TightMode::parse(control, encoding)?;
        if matches!(mode, TightMode::Fill) {
            return Ok(TightDecodedPixels::Fill(read_tight_pixel(
                reader,
                self.pixel_format,
            )?));
        }
        if mode.is_image() {
            let length = reader.read_compact_length()?;
            let encoded = reader.read_exact(length)?;
            let format = if matches!(mode, TightMode::Jpeg) {
                image::ImageFormat::Jpeg
            } else {
                image::ImageFormat::Png
            };
            use image::ImageDecoder;
            let mut image_reader =
                image::ImageReader::with_format(std::io::Cursor::new(encoded), format);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(rect.width);
            limits.max_image_height = Some(rect.height);
            limits.max_alloc = Some(super::rfb_limits::MAX_FRAMEBUFFER_BYTES as u64);
            image_reader.limits(limits);
            let decoder = image_reader.into_decoder().map_err(|error| match error {
                image::ImageError::Limits(_) => "rfb_resource_limit:tight_image",
                _ => "rfb_tight_image_decode_failed",
            })?;
            // Validate the image header before allocating or decoding its pixels.
            if decoder.dimensions() != (rect.width, rect.height) {
                return Err("rfb_tight_image_geometry_mismatch".into());
            }
            if decoder.total_bytes() > super::rfb_limits::MAX_FRAMEBUFFER_BYTES as u64 {
                return Err("rfb_resource_limit:tight_image".into());
            }
            let image = image::DynamicImage::from_decoder(decoder)
                .map_err(|error| match error {
                    image::ImageError::Limits(_) => "rfb_resource_limit:tight_image",
                    _ => "rfb_tight_image_decode_failed",
                })?
                .to_rgba8();
            let mut pixels = image.into_raw();
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            return Ok(TightDecodedPixels::Pixels(pixels));
        }
        let TightMode::Basic {
            stream: stream_id,
            explicit_filter,
            no_zlib,
        } = mode
        else {
            unreachable!("Fill and image modes return before Basic")
        };
        let filter = if explicit_filter {
            reader.read_u8()?
        } else {
            0
        };
        let width = rect.width as usize;
        let height = rect.height as usize;
        let pixel_bytes = self.pixel_format.tight_pixel_bytes();
        let mut _palette_memory = None;
        let (expected, palette) = match filter {
            0 | 2 => (
                super::rfb_tight::filtered_length(filter, width, height, pixel_bytes, 0)?,
                Vec::new(),
            ),
            1 => {
                let count = usize::from(reader.read_u8()?) + 1;
                let expected =
                    super::rfb_tight::filtered_length(filter, width, height, pixel_bytes, count)?;
                _palette_memory = Some(self.memory.reserve(count * 4)?);
                let palette = (0..count)
                    .map(|_| read_tight_pixel(reader, self.pixel_format))
                    .collect::<Result<Vec<_>, _>>()?;
                (expected, palette)
            }
            _ => return Err("rfb_unsupported_tight_filter".into()),
        };
        let filtered = if expected < 12 {
            reader.read_exact(expected)?.to_vec()
        } else {
            let length = reader.read_compact_length()?;
            let compressed = reader.read_exact(length)?;
            if no_zlib {
                if length != expected {
                    return Err(format!(
                        "rfb_tight_decoded_length_mismatch:no_zlib_decode,{stream_id},{width},{height},{length},0,{expected},{length}",
                    ));
                }
                compressed.to_vec()
            } else {
                decompress_tight_stream(
                    &mut self.tight_decoders[stream_id],
                    compressed,
                    expected,
                    stream_id,
                    rect,
                )?
            }
        };
        match filter {
            0 => tight_copy_to_bgra(&filtered, self.pixel_format),
            1 => tight_palette_to_bgra(
                &filtered,
                &palette,
                rect.width as usize,
                rect.height as usize,
            ),
            2 => tight_gradient_to_bgra(filtered, width, height, self.pixel_format),
            _ => unreachable!(),
        }
        .map(TightDecodedPixels::Pixels)
    }
}

fn decompress_tight_stream(
    decoder: &mut Decompress,
    compressed: &[u8],
    expected: usize,
    stream_id: usize,
    rect: FramebufferRect,
) -> Result<Vec<u8>, String> {
    let before = decoder.total_in();
    // One guard byte allows consuming the flush trailer at an exact output
    // boundary and detects excess decoded data without unbounded inflation.
    let mut output = Vec::with_capacity(expected + 1);
    decoder
        .decompress_vec(compressed, &mut output, FlushDecompress::Sync)
        .map_err(|_| "rfb_tight_decompression_failed")?;
    if decoder.total_in() - before != compressed.len() as u64 || output.len() != expected {
        // The failure boundary retains only this numeric suffix, never pixels.
        return Err(format!(
            "rfb_tight_decoded_length_mismatch:inflate,{stream_id},{},{},{},{},{expected},{}",
            rect.width,
            rect.height,
            compressed.len(),
            decoder.total_in() - before,
            output.len(),
        ));
    }
    Ok(output)
}

fn read_tight_pixel(
    reader: &mut MessageReader<'_>,
    format: ClientRfbPixelFormat,
) -> Result<[u8; 4], String> {
    let pixel = reader.read_exact(format.tight_pixel_bytes())?;
    if pixel.len() == 3 {
        Ok([pixel[2], pixel[1], pixel[0], 255])
    } else {
        Ok(decode_wire_pixel(
            format,
            pixel.try_into().expect("validated 32-bit pixel"),
        ))
    }
}

fn tight_copy_to_bgra(filtered: &[u8], format: ClientRfbPixelFormat) -> Result<Vec<u8>, String> {
    let pixel_bytes = format.tight_pixel_bytes();
    if !filtered.len().is_multiple_of(pixel_bytes) {
        return Err("rfb_tight_copy_length_mismatch".into());
    }
    let mut pixels = Vec::with_capacity(filtered.len() / pixel_bytes * 4);
    if pixel_bytes == 3 {
        for rgb in filtered.as_chunks::<3>().0 {
            pixels.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
        }
    } else {
        for pixel in filtered.as_chunks::<4>().0 {
            pixels.extend_from_slice(&decode_wire_pixel(format, *pixel));
        }
    }
    Ok(pixels)
}

fn tight_palette_to_bgra(
    filtered: &[u8],
    palette: &[[u8; 4]],
    width: usize,
    height: usize,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(width * height * 4);
    if palette.len() == 2 {
        let row_bytes = width.div_ceil(8);
        if filtered.len() != row_bytes * height {
            return Err("rfb_tight_palette_length_mismatch".into());
        }
        for row in filtered.chunks_exact(row_bytes) {
            for x in 0..width {
                output.extend_from_slice(&palette[usize::from((row[x / 8] >> (7 - x % 8)) & 1)]);
            }
        }
    } else {
        if filtered.len() != width * height {
            return Err("rfb_tight_palette_length_mismatch".into());
        }
        for index in filtered {
            output.extend_from_slice(
                palette
                    .get(usize::from(*index))
                    .ok_or("rfb_tight_palette_index_out_of_bounds")?,
            );
        }
    }
    Ok(output)
}

fn tight_gradient_to_bgra(
    mut filtered: Vec<u8>,
    width: usize,
    height: usize,
    format: ClientRfbPixelFormat,
) -> Result<Vec<u8>, String> {
    let pixel_bytes = format.tight_pixel_bytes();
    if filtered.len() != width * height * pixel_bytes {
        return Err("rfb_tight_gradient_length_mismatch".into());
    }
    for y in 0..height {
        for x in 0..width {
            let offset = (y * width + x) * pixel_bytes;
            if pixel_bytes == 4 {
                let value_at = |index: usize| {
                    let pixel = filtered[index..index + 4]
                        .try_into()
                        .expect("complete pixel");
                    if format.big_endian {
                        u32::from_be_bytes(pixel)
                    } else {
                        u32::from_le_bytes(pixel)
                    }
                };
                let delta = value_at(offset);
                let left = if x == 0 { 0 } else { value_at(offset - 4) };
                let up = if y == 0 {
                    0
                } else {
                    value_at(offset - width * 4)
                };
                let upper_left = if x == 0 || y == 0 {
                    0
                } else {
                    value_at(offset - width * 4 - 4)
                };
                let mut value = 0;
                for (max, shift) in [
                    (format.red_max, format.red_shift),
                    (format.green_max, format.green_shift),
                    (format.blue_max, format.blue_shift),
                ] {
                    let max = u32::from(max);
                    let component = |pixel: u32| ((pixel >> shift) & max) as i32;
                    let predictor = (component(left) + component(up) - component(upper_left))
                        .clamp(0, max as i32) as u32;
                    value |= ((((delta >> shift) & max) + predictor) & max) << shift;
                }
                let pixel = if format.big_endian {
                    value.to_be_bytes()
                } else {
                    value.to_le_bytes()
                };
                filtered[offset..offset + 4].copy_from_slice(&pixel);
                continue;
            }
            for component in 0..3 {
                let offset = offset + component;
                let left = if x == 0 { 0 } else { filtered[offset - 3] };
                let up = if y == 0 {
                    0
                } else {
                    filtered[offset - width * 3]
                };
                let upper_left = if x == 0 || y == 0 {
                    0
                } else {
                    filtered[offset - width * 3 - 3]
                };
                let predictor =
                    (i16::from(left) + i16::from(up) - i16::from(upper_left)).clamp(0, 255) as u8;
                filtered[offset] = filtered[offset].wrapping_add(predictor);
            }
        }
    }
    tight_copy_to_bgra(&filtered, format)
}
