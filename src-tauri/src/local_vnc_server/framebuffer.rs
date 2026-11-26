use crate::local_vnc_server::{
    CapturedPixelFormat, DisplaySize, LocalVncServerRefreshMode, LocalVncServerResolutionMode,
    RfbPixelFormat,
};
use std::time::Duration;

const MAX_FRAMEBUFFER_WIDTH: u32 = 1920;
const MAX_FRAMEBUFFER_HEIGHT: u32 = 1080;

pub fn default_rfb_pixel_format() -> RfbPixelFormat {
    RfbPixelFormat {
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

pub fn refresh_interval(refresh_mode: LocalVncServerRefreshMode) -> Duration {
    match refresh_mode {
        LocalVncServerRefreshMode::Responsive => Duration::from_millis(16),
        LocalVncServerRefreshMode::Balanced => Duration::from_millis(125),
        LocalVncServerRefreshMode::LowPower => Duration::from_millis(333),
    }
}

pub fn convert_rfb_framebuffer_to_pixel_format(
    framebuffer: &[u8],
    pixel_format: RfbPixelFormat,
) -> Vec<u8> {
    if pixel_format == default_rfb_pixel_format() || pixel_format.bits_per_pixel != 32 {
        return framebuffer.to_vec();
    }

    if !pixel_format.true_color {
        return framebuffer.to_vec();
    }

    let mut converted = Vec::with_capacity(framebuffer.len());
    for pixel in framebuffer.as_chunks::<4>().0 {
        let red = scale_channel(pixel[2], pixel_format.red_max);
        let green = scale_channel(pixel[1], pixel_format.green_max);
        let blue = scale_channel(pixel[0], pixel_format.blue_max);
        let value = (red << pixel_format.red_shift)
            | (green << pixel_format.green_shift)
            | (blue << pixel_format.blue_shift);
        let bytes = if pixel_format.big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        converted.extend_from_slice(&bytes);
    }

    converted
}

fn scale_channel(value: u8, max: u16) -> u32 {
    if max == 255 {
        value as u32
    } else {
        ((value as u32 * max as u32) / 255).min(max as u32)
    }
}

pub fn scale_rgba_to_rfb_framebuffer(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    source_stride: usize,
    target_width: usize,
    target_height: usize,
) -> Vec<u8> {
    scale_captured_framebuffer_to_rfb(
        source,
        source_width,
        source_height,
        source_stride,
        target_width,
        target_height,
        CapturedPixelFormat::Rgba8888,
    )
}

pub fn scale_captured_framebuffer_to_rfb(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    source_stride: usize,
    target_width: usize,
    target_height: usize,
    pixel_format: CapturedPixelFormat,
) -> Vec<u8> {
    let mut target = Vec::with_capacity(target_width * target_height * 4);

    for y in 0..target_height {
        let source_y = y * source_height / target_height;
        for x in 0..target_width {
            let source_x = x * source_width / target_width;
            let offset = source_y
                .saturating_mul(source_stride)
                .saturating_add(source_x.saturating_mul(4));
            if offset + 3 < source.len() {
                let (red, green, blue) = match pixel_format {
                    CapturedPixelFormat::Rgba8888 => {
                        (source[offset], source[offset + 1], source[offset + 2])
                    }
                    CapturedPixelFormat::Bgra8888 => {
                        (source[offset + 2], source[offset + 1], source[offset])
                    }
                    CapturedPixelFormat::Argb8888 => {
                        (source[offset + 1], source[offset + 2], source[offset + 3])
                    }
                    CapturedPixelFormat::Abgr8888 => {
                        (source[offset + 3], source[offset + 2], source[offset + 1])
                    }
                };
                target.extend_from_slice(&[blue, green, red, 0]);
            } else {
                target.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    target
}

pub fn constrained_framebuffer_size(pixel_width: u32, pixel_height: u32) -> DisplaySize {
    fit_framebuffer_size(
        pixel_width,
        pixel_height,
        MAX_FRAMEBUFFER_WIDTH,
        MAX_FRAMEBUFFER_HEIGHT,
    )
}

pub fn framebuffer_size_for_resolution(
    pixel_width: u32,
    pixel_height: u32,
    resolution_mode: LocalVncServerResolutionMode,
) -> DisplaySize {
    match resolution_mode {
        LocalVncServerResolutionMode::Fit => {
            constrained_framebuffer_size(pixel_width, pixel_height)
        }
        LocalVncServerResolutionMode::Native => DisplaySize {
            width: pixel_width.max(1).min(u16::MAX as u32),
            height: pixel_height.max(1).min(u16::MAX as u32),
        },
        LocalVncServerResolutionMode::Hd => {
            fit_framebuffer_size(pixel_width, pixel_height, 1280, 720)
        }
        LocalVncServerResolutionMode::Fhd => {
            fit_framebuffer_size(pixel_width, pixel_height, 1920, 1080)
        }
        LocalVncServerResolutionMode::HdLandscape => DisplaySize {
            width: 1280,
            height: 720,
        },
        LocalVncServerResolutionMode::HdPortrait => DisplaySize {
            width: 720,
            height: 1280,
        },
        LocalVncServerResolutionMode::FhdLandscape => DisplaySize {
            width: 1920,
            height: 1080,
        },
        LocalVncServerResolutionMode::FhdPortrait => DisplaySize {
            width: 1080,
            height: 1920,
        },
    }
}

fn fit_framebuffer_size(
    pixel_width: u32,
    pixel_height: u32,
    max_width: u32,
    max_height: u32,
) -> DisplaySize {
    let width = pixel_width.max(1);
    let height = pixel_height.max(1);
    let scale = (max_width as f64 / width as f64)
        .min(max_height as f64 / height as f64)
        .min(1.0);

    DisplaySize {
        width: ((width as f64 * scale).round() as u32).max(1),
        height: ((height as f64 * scale).round() as u32).max(1),
    }
}
