use super::*;
use crate::remote_desktop_core::{memory::ClientMemoryBudget, PixelFormat};
use std::hint::black_box;
use std::io::Write;
use std::time::Instant;

const WARMUP: usize = 5;
const SAMPLES: usize = 30;

#[derive(Clone, Copy)]
enum Case {
    Raw,
    CopyRect,
    HextileRaw,
    Zlib,
    ZrleSolid,
    TightBasic,
    TightFill,
    TightPng,
    TightJpeg,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::CopyRect => "copy_rect",
            Self::HextileRaw => "hextile_raw",
            Self::Zlib => "zlib",
            Self::ZrleSolid => "zrle_solid",
            Self::TightBasic => "tight_basic",
            Self::TightFill => "tight_fill",
            Self::TightPng => "tight_png",
            Self::TightJpeg => "tight_jpeg",
        }
    }
}

fn rectangle(x: u16, y: u16, width: u16, height: u16, encoding: i32, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12 + body.len());
    for value in [x, y, width, height] {
        bytes.extend(value.to_be_bytes());
    }
    bytes.extend(encoding.to_be_bytes());
    bytes.extend(body);
    bytes
}

fn update(rectangles: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + rectangles.iter().map(Vec::len).sum::<usize>());
    bytes.extend([0, 0]);
    bytes.extend(u16::try_from(rectangles.len()).unwrap().to_be_bytes());
    for rectangle in rectangles {
        bytes.extend(rectangle);
    }
    bytes
}

fn raw(width: u16, height: u16, gray: u8) -> Vec<u8> {
    update(&[rectangle(
        0,
        0,
        width,
        height,
        0,
        &[gray, gray, gray, 255].repeat(usize::from(width) * usize::from(height)),
    )])
}

fn compact_length(mut length: usize) -> Vec<u8> {
    assert!(length <= 0x3fffff);
    let mut bytes = vec![(length & 0x7f) as u8];
    length >>= 7;
    if length != 0 {
        bytes[0] |= 0x80;
        bytes.push((length & 0x7f) as u8);
        length >>= 7;
        if length != 0 {
            bytes[1] |= 0x80;
            bytes.push(length as u8);
        }
    }
    bytes
}

fn sync_chunk(encoder: &mut flate2::write::ZlibEncoder<Vec<u8>>, bytes: &[u8]) -> Vec<u8> {
    let offset = encoder.get_ref().len();
    encoder.write_all(bytes).unwrap();
    encoder.flush().unwrap();
    encoder.get_ref()[offset..].to_vec()
}

// All data is synthetic. Generic zlib/image codecs only wrap independently
// assembled RFB rectangles; no production Server writer or screen capture runs.
fn fixtures(case: Case, width: u16, height: u16) -> Vec<Vec<u8>> {
    let pixels = usize::from(width) * usize::from(height);
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    match case {
        Case::Raw => vec![raw(width, height, 128)],
        Case::CopyRect => vec![update(&[rectangle(
            1,
            0,
            width - 1,
            height,
            1,
            &[0, 0, 0, 0],
        )])],
        Case::HextileRaw => {
            let mut body = Vec::with_capacity(
                pixels * 4 + usize::from(width).div_ceil(16) * usize::from(height).div_ceil(16),
            );
            for y in (0..height).step_by(16) {
                for x in (0..width).step_by(16) {
                    body.push(1);
                    body.extend([128, 128, 128, 255].repeat(
                        usize::from((width - x).min(16)) * usize::from((height - y).min(16)),
                    ));
                }
            }
            vec![update(&[rectangle(0, 0, width, height, 5, &body)])]
        }
        Case::Zlib | Case::ZrleSolid => {
            let (encoding, plain) = if matches!(case, Case::Zlib) {
                (6, [128, 128, 128, 255].repeat(pixels))
            } else {
                (
                    16,
                    [1, 128, 128, 128]
                        .repeat(usize::from(width).div_ceil(64) * usize::from(height).div_ceil(64)),
                )
            };
            (0..WARMUP + SAMPLES)
                .map(|_| {
                    let chunk = sync_chunk(&mut encoder, &plain);
                    let body = [(chunk.len() as u32).to_be_bytes().as_slice(), &chunk].concat();
                    update(&[rectangle(0, 0, width, height, encoding, &body)])
                })
                .collect()
        }
        Case::TightBasic | Case::TightFill | Case::TightPng | Case::TightJpeg => {
            let rounds = if matches!(case, Case::TightBasic) {
                WARMUP + SAMPLES
            } else {
                1
            };
            (0..rounds)
                .map(|_| {
                    let mut rectangles = Vec::new();
                    // Width follows Tight's 2048 limit. 512 rows also keep even
                    // uncompressed RGB below the three-byte compact-length ceiling.
                    for y in (0..height).step_by(512) {
                        for x in (0..width).step_by(2048) {
                            let w = (width - x).min(2048);
                            let h = (height - y).min(512);
                            let (encoding, body) = match case {
                                Case::TightFill => (7, vec![0x80, 128, 128, 128]),
                                Case::TightBasic => {
                                    let plain = vec![128; usize::from(w) * usize::from(h) * 3];
                                    let chunk = sync_chunk(&mut encoder, &plain);
                                    (7, [&[0][..], &compact_length(chunk.len()), &chunk].concat())
                                }
                                Case::TightPng | Case::TightJpeg => {
                                    use image::ImageEncoder;
                                    let plain = vec![128; usize::from(w) * usize::from(h) * 3];
                                    let mut encoded = Vec::new();
                                    if matches!(case, Case::TightPng) {
                                        image::codecs::png::PngEncoder::new(&mut encoded)
                                            .write_image(
                                                &plain,
                                                u32::from(w),
                                                u32::from(h),
                                                image::ExtendedColorType::Rgb8,
                                            )
                                            .unwrap();
                                    } else {
                                        image::codecs::jpeg::JpegEncoder::new_with_quality(
                                            &mut encoded,
                                            95,
                                        )
                                        .encode(
                                            &plain,
                                            u32::from(w),
                                            u32::from(h),
                                            image::ExtendedColorType::Rgb8,
                                        )
                                        .unwrap();
                                    }
                                    let control = if matches!(case, Case::TightPng) {
                                        0xa0
                                    } else {
                                        0x90
                                    };
                                    let encoding = if matches!(case, Case::TightPng) {
                                        -260
                                    } else {
                                        7
                                    };
                                    (
                                        encoding,
                                        [&[control][..], &compact_length(encoded.len()), &encoded]
                                            .concat(),
                                    )
                                }
                                _ => unreachable!(),
                            };
                            rectangles.push(rectangle(x, y, w, h, encoding, &body));
                        }
                    }
                    update(&rectangles)
                })
                .collect()
        }
    }
}

#[test]
#[ignore = "release-only synthetic decoder/memory benchmark; see Session README"]
fn rfb_decode_budget_benchmark() -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("run this benchmark with cargo test --release".into());
    }
    println!(
        "rfb_benchmark_meta={}",
        serde_json::json!({
            "arch": std::env::consts::ARCH, "os": std::env::consts::OS,
            "warmup_updates": WARMUP, "sample_updates": SAMPLES,
            "input_pattern": "synthetic uniform gray; CopyRect shifts black baseline",
            "entry": "decode_to_hub; excludes receive, Surface, Canvas and native workers",
            "memory_units": "bytes; shared admission includes allowance and conservative scratch",
            "allocation_scope": "BudgetedBytes only; excludes scratch Vec, codecs and fixtures"
        })
    );
    for (width, height) in [(1920_u16, 1080_u16), (3840, 2160), (7680, 2160)] {
        for case in [
            Case::Raw,
            Case::CopyRect,
            Case::HextileRaw,
            Case::Zlib,
            Case::ZrleSolid,
            Case::TightBasic,
            Case::TightFill,
            Case::TightPng,
            Case::TightJpeg,
        ] {
            let messages = fixtures(case, width, height);
            let fixture_retained_bytes: usize = messages.iter().map(Vec::capacity).sum::<usize>()
                + messages.capacity() * std::mem::size_of::<Vec<u8>>();
            let geometry = FramebufferGeometry {
                width: u32::from(width),
                height: u32::from(height),
                pixel_format: PixelFormat::Bgra8888,
            };
            let memory = ClientMemoryBudget::default();
            let baseline = memory.in_use();
            let hub = ClientFramebufferHub::with_memory(memory.clone());
            let mut decoder = RfbClientFramebufferDecoder::with_memory(
                ClientRfbPixelFormat::core_bgra32(),
                memory.clone(),
            )
            .unwrap();
            decoder
                .decode_to_hub(&raw(width, height, 0), geometry, &hub)
                .unwrap();
            for index in 0..WARMUP {
                decoder
                    .decode_to_hub(&messages[index % messages.len()], geometry, &hub)
                    .unwrap();
            }
            let warm_admission = memory.in_use();
            let warm_allocations = memory.budgeted_buffer_allocations();
            assert_eq!(
                warm_allocations, 2,
                "published and working frame allocations"
            );
            let mut times = Vec::with_capacity(SAMPLES);
            let mut wire_bytes = 0;
            for index in WARMUP..WARMUP + SAMPLES {
                let message = &messages[index % messages.len()];
                let started = Instant::now();
                let outcome = decoder
                    .decode_to_hub(black_box(message), geometry, &hub)
                    .unwrap();
                times.push(started.elapsed().as_micros() as u64);
                assert_eq!(outcome.frame_id, Some(index as u64 + 2));
                wire_bytes += message.len();
            }
            let final_admission = memory.in_use();
            let final_allocations = memory.budgeted_buffer_allocations();
            let peak = memory.peak_in_use();
            assert_eq!(warm_admission, final_admission);
            assert_eq!(
                warm_allocations, final_allocations,
                "no BudgetedBytes growth after warmup"
            );
            let expected = if matches!(case, Case::CopyRect) {
                [0, 0, 0, 255]
            } else {
                [128, 128, 128, 255]
            };
            {
                let frame = hub.lock().unwrap();
                assert_eq!(frame.geometry(), Some(geometry));
                assert_eq!(
                    frame.pixels().len(),
                    usize::from(width) * usize::from(height) * 4
                );
                assert!(frame
                    .pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == expected));
            }
            drop(decoder);
            drop(hub);
            assert_eq!(memory.in_use(), baseline);
            times.sort_unstable();
            println!(
                "rfb_benchmark={}",
                serde_json::json!({
                    "case": case.name(), "width": width, "height": height,
                    "canonical_frame_bytes": usize::from(width) * usize::from(height) * 4,
                    "samples": SAMPLES, "decode_p50_us": times[SAMPLES.div_ceil(2) - 1],
                    "decode_p95_us": times[(SAMPLES * 95).div_ceil(100) - 1],
                    "decode_max_us": times[SAMPLES - 1], "mean_wire_bytes": wire_bytes / SAMPLES,
                    "fixture_retained_bytes": fixture_retained_bytes, "baseline_admission_bytes": baseline,
                    "shared_admission_after_warm_bytes": warm_admission,
                    "shared_admission_peak_bytes": peak, "peak_minus_baseline_bytes": peak - baseline,
                    "budgeted_buffer_allocations": final_allocations,
                    "budgeted_buffer_allocations_during_samples": final_allocations - warm_allocations,
                    "after_drop_admission_bytes": memory.in_use()
                })
            );
        }
    }
    Ok(())
}
