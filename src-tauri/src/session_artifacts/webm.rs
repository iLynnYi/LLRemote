use std::io::{Read, Seek, SeekFrom, Write};

const SEGMENT: u32 = 0x18538067;
const INFO: u32 = 0x1549a966;
const TRACKS: u32 = 0x1654ae6b;
const CLUSTER: u32 = 0x1f43b675;
const CUES: u32 = 0x1c53bb6b;
const SEEK_HEAD: u32 = 0x114d9b74;
const INVALID: &str = "recording_invalid_webm";

#[derive(Clone, Copy)]
struct Element {
    id: u32,
    start: u64,
    data: u64,
    end: u64,
    unknown: bool,
}
struct Cue {
    time: u64,
    track: u64,
    relative: u64,
}

fn io<T>(result: std::io::Result<T>) -> Result<T, String> {
    result.map_err(|_| "recording_io_failed".into())
}

fn vint<R: Read>(reader: &mut R, id: bool) -> Result<(u64, usize, bool), String> {
    let mut first = [0];
    io(reader.read_exact(&mut first))?;
    if first[0] == 0 {
        return Err(INVALID.into());
    }
    let length = first[0].leading_zeros() as usize + 1;
    if length > if id { 4 } else { 8 } {
        return Err(INVALID.into());
    }
    let mut value = if id {
        u64::from(first[0])
    } else {
        u64::from(first[0]) & (0xff_u64 >> length)
    };
    for _ in 1..length {
        let mut byte = [0];
        io(reader.read_exact(&mut byte))?;
        value = (value << 8) | u64::from(byte[0]);
    }
    Ok((value, length, !id && value == (1u64 << (length * 7)) - 1))
}

fn element<R: Read + Seek>(reader: &mut R, start: u64, end: u64) -> Result<Element, String> {
    if start >= end {
        return Err(INVALID.into());
    }
    io(reader.seek(SeekFrom::Start(start)))?;
    let (id, _, _) = vint(reader, true)?;
    let (size, _, unknown) = vint(reader, false)?;
    let data = io(reader.stream_position())?;
    let stop = if unknown {
        end
    } else {
        data.checked_add(size).ok_or(INVALID)?
    };
    if data > end || stop > end {
        return Err(INVALID.into());
    }
    Ok(Element {
        id: id as u32,
        start,
        data,
        end: stop,
        unknown,
    })
}

fn children<R: Read + Seek>(reader: &mut R, start: u64, end: u64) -> Result<Vec<Element>, String> {
    let mut elements = Vec::new();
    let mut offset = start;
    while offset < end {
        if elements.len() >= 1_000_000 {
            return Err("recording_index_limit".into());
        }
        let child = element(reader, offset, end)?;
        if child.unknown {
            return Err(INVALID.into());
        }
        offset = child.end;
        elements.push(child);
    }
    Ok(elements)
}

fn is_level_one(id: u32) -> bool {
    matches!(
        id,
        INFO | TRACKS | CLUSTER | CUES | SEEK_HEAD | 0x1254c367 | 0x1941a469 | 0x1043a770
    )
}

fn segments<R: Read + Seek>(reader: &mut R, segment: Element) -> Result<Vec<Element>, String> {
    let mut result = Vec::new();
    let mut offset = segment.data;
    while offset < segment.end {
        if result.len() >= 1_000_000 {
            return Err("recording_index_limit".into());
        }
        let mut child = element(reader, offset, segment.end)?;
        if child.unknown {
            if child.id != CLUSTER {
                return Err(INVALID.into());
            }
            let mut position = child.data;
            while position < segment.end {
                let nested = element(reader, position, segment.end)?;
                if is_level_one(nested.id) {
                    break;
                }
                if nested.unknown {
                    return Err(INVALID.into());
                }
                position = nested.end;
            }
            child.end = position;
        }
        offset = child.end;
        result.push(child);
    }
    Ok(result)
}

fn payload<R: Read + Seek>(reader: &mut R, item: Element) -> Result<Vec<u8>, String> {
    if item.end - item.data > 1024 * 1024 {
        return Err("recording_metadata_limit".into());
    }
    io(reader.seek(SeekFrom::Start(item.data)))?;
    let mut bytes = vec![0; (item.end - item.data) as usize];
    io(reader.read_exact(&mut bytes))?;
    Ok(bytes)
}

fn unsigned<R: Read + Seek>(reader: &mut R, item: Element) -> Result<u64, String> {
    if item.end - item.data > 8 {
        return Err(INVALID.into());
    }
    Ok(payload(reader, item)?
        .into_iter()
        .fold(0, |value, byte| (value << 8) | u64::from(byte)))
}

fn size(value: u64) -> [u8; 8] {
    (value | (1 << 56)).to_be_bytes()
}
fn id_bytes(id: u32) -> Vec<u8> {
    id.to_be_bytes()
        .into_iter()
        .skip_while(|byte| *byte == 0)
        .collect()
}
fn encoded(id: u32, bytes: &[u8]) -> Vec<u8> {
    [
        id_bytes(id),
        size(bytes.len() as u64).to_vec(),
        bytes.to_vec(),
    ]
    .concat()
}
fn uint(id: u32, value: u64) -> Vec<u8> {
    encoded(id, &value.to_be_bytes())
}

fn copy_range<R: Read + Seek, W: Write>(
    reader: &mut R,
    output: &mut W,
    start: u64,
    end: u64,
) -> Result<(), String> {
    io(reader.seek(SeekFrom::Start(start)))?;
    let count = io(std::io::copy(&mut reader.take(end - start), output))?;
    if count != end - start {
        return Err(INVALID.into());
    }
    Ok(())
}

fn video_track<R: Read + Seek>(reader: &mut R, tracks: Element) -> Result<u64, String> {
    for entry in children(reader, tracks.data, tracks.end)? {
        if entry.id != 0xae {
            continue;
        }
        let fields = children(reader, entry.data, entry.end)?;
        let number = fields
            .iter()
            .find(|field| field.id == 0xd7)
            .ok_or(INVALID)?;
        let kind = fields
            .iter()
            .find(|field| field.id == 0x83)
            .ok_or(INVALID)?;
        if unsigned(reader, *kind)? == 1 {
            return unsigned(reader, *number);
        }
    }
    Err(INVALID.into())
}

fn cluster_cues<R: Read + Seek>(
    reader: &mut R,
    cluster: Element,
    track: u64,
) -> Result<Vec<Cue>, String> {
    let fields = children(reader, cluster.data, cluster.end)?;
    let timestamp = match fields.iter().find(|field| field.id == 0xe7) {
        Some(field) => unsigned(reader, *field)?,
        None => 0,
    };
    let mut cues = Vec::new();
    for field in fields {
        let (block, key) = if field.id == 0xa3 {
            (field, None)
        } else if field.id == 0xa0 {
            let group = children(reader, field.data, field.end)?;
            let Some(block) = group.iter().find(|item| item.id == 0xa1) else {
                continue;
            };
            (*block, Some(!group.iter().any(|item| item.id == 0xfb)))
        } else {
            continue;
        };
        io(reader.seek(SeekFrom::Start(block.data)))?;
        let (block_track, length, _) = vint(reader, false)?;
        if block.data + length as u64 + 3 > block.end {
            return Err(INVALID.into());
        }
        let mut header = [0; 3];
        io(reader.read_exact(&mut header))?;
        if block_track != track || !key.unwrap_or(header[2] & 0x80 != 0) {
            continue;
        }
        let time = timestamp as i128 + i128::from(i16::from_be_bytes([header[0], header[1]]));
        if time < 0 || time > u64::MAX as i128 {
            return Err(INVALID.into());
        }
        cues.push(Cue {
            time: time as u64,
            track,
            relative: field.start - cluster.data,
        });
    }
    Ok(cues)
}

fn seek_head(info: u64, tracks: u64, cues: u64) -> Vec<u8> {
    let mut entries = Vec::new();
    for (id, position) in [(INFO, info), (TRACKS, tracks), (CUES, cues)] {
        entries.extend(encoded(
            0x4dbb,
            &[encoded(0x53ab, &id_bytes(id)), uint(0x53ac, position)].concat(),
        ));
    }
    encoded(SEEK_HEAD, &entries)
}

/// A single encoder is kept running across file boundaries. Every input begins at
/// a video keyframe; timestamps are rebased while blocks are copied unchanged.
pub fn finalize_webm<R: Read + Seek, W: Write + Seek>(
    input: &mut R,
    output: &mut W,
    duration_ms: f64,
) -> Result<(), String> {
    merge_webm(&mut [input], output, &[duration_ms], &mut |_| {})
}

struct Layout {
    header: Element,
    segment: Element,
    elements: Vec<Element>,
    info: Element,
    tracks: Element,
    track: u64,
    scale: u64,
    origin: u64,
}
fn layout<R: Read + Seek>(input: &mut R) -> Result<Layout, String> {
    let length = io(input.seek(SeekFrom::End(0)))?;
    let header = element(input, 0, length)?;
    if header.id != 0x1a45dfa3 || header.unknown {
        return Err(INVALID.into());
    }
    let segment = element(input, header.end, length)?;
    if segment.id != SEGMENT {
        return Err(INVALID.into());
    }
    let elements = segments(input, segment)?;
    let info = *elements
        .iter()
        .find(|item| item.id == INFO)
        .ok_or(INVALID)?;
    let tracks = *elements
        .iter()
        .find(|item| item.id == TRACKS)
        .ok_or(INVALID)?;
    let track = video_track(input, tracks)?;
    let fields = children(input, info.data, info.end)?;
    let scale = match fields.iter().find(|item| item.id == 0x2ad7b1) {
        Some(item) => unsigned(input, *item)?,
        None => 1_000_000,
    };
    if scale == 0 {
        return Err(INVALID.into());
    }
    let first = *elements
        .iter()
        .find(|item| item.id == CLUSTER)
        .ok_or(INVALID)?;
    let fields = children(input, first.data, first.end)?;
    let origin = match fields.iter().find(|item| item.id == 0xe7) {
        Some(item) => unsigned(input, *item)?,
        None => 0,
    };
    Ok(Layout {
        header,
        segment,
        elements,
        info,
        tracks,
        track,
        scale,
        origin,
    })
}

/// Remuxes independent segments without decoding or restarting the encoder.
/// The caller supplies media durations, not wall-clock time spent saving files.
pub fn merge_webm<R: Read + Seek, W: Write + Seek>(
    inputs: &mut [R],
    output: &mut W,
    durations_ms: &[f64],
    progress: &mut dyn FnMut(u64),
) -> Result<(), String> {
    if inputs.is_empty()
        || inputs.len() != durations_ms.len()
        || durations_ms.iter().any(|v| !v.is_finite() || *v <= 0.0)
    {
        return Err("recording_invalid_duration".into());
    }
    let first = layout(&mut inputs[0])?;
    let tracks = payload(&mut inputs[0], first.tracks)?;
    let mut info = Vec::new();
    for item in children(&mut inputs[0], first.info.data, first.info.end)? {
        if !matches!(item.id, 0x4489 | 0xbf) {
            copy_range(&mut inputs[0], &mut info, item.start, item.end)?;
        }
    }
    info.extend(encoded(
        0x4489,
        &(durations_ms.iter().sum::<f64>() * 1_000_000.0 / first.scale as f64).to_be_bytes(),
    ));
    copy_range(&mut inputs[0], output, 0, first.header.end)?;
    io(output.write_all(&id_bytes(SEGMENT)))?;
    let size_position = io(output.stream_position())?;
    io(output.write_all(&size(0)))?;
    let base = io(output.stream_position())?;
    io(output.write_all(&seek_head(0, 0, 0)))?;
    let info_position = io(output.stream_position())? - base;
    io(output.write_all(&encoded(INFO, &info)))?;
    let tracks_position = io(output.stream_position())? - base;
    io(output.write_all(&encoded(TRACKS, &tracks)))?;
    let mut cues = Vec::new();
    let mut offset_ms = 0.0;
    for (index, input) in inputs.iter_mut().enumerate() {
        let current = layout(input)?;
        if current.scale != first.scale || payload(input, current.tracks)? != tracks {
            return Err("recording_incompatible_segments".into());
        }
        let offset = (offset_ms * 1_000_000.0 / first.scale as f64).round() as u64;
        for cluster in current.elements.iter().filter(|item| item.id == CLUSTER) {
            let fields = children(input, cluster.data, cluster.end)?;
            let timestamp = match fields.iter().find(|item| item.id == 0xe7) {
                Some(item) => unsigned(input, *item)?,
                None => 0,
            };
            let time = timestamp
                .checked_sub(current.origin)
                .and_then(|v| v.checked_add(offset))
                .ok_or(INVALID)?;
            let time_field = uint(0xe7, time);
            let retained: Vec<_> = fields
                .iter()
                .filter(|item| !matches!(item.id, 0xe7 | 0xbf))
                .collect();
            let cluster_position = io(output.stream_position())? - base;
            let content_size = time_field.len() as u64
                + retained
                    .iter()
                    .map(|item| item.end - item.start)
                    .sum::<u64>();
            io(output.write_all(&id_bytes(CLUSTER)))?;
            io(output.write_all(&size(content_size)))?;
            io(output.write_all(&time_field))?;
            let source_cues = cluster_cues(input, *cluster, current.track)?;
            let mut relative = time_field.len() as u64;
            for item in retained {
                for cue in source_cues
                    .iter()
                    .filter(|cue| cue.relative == item.start - cluster.data)
                {
                    if cues.len() > 16 * 1024 * 1024 {
                        return Err("recording_index_limit".into());
                    }
                    let cue_time = cue
                        .time
                        .checked_sub(current.origin)
                        .and_then(|v| v.checked_add(offset))
                        .ok_or(INVALID)?;
                    let location = encoded(
                        0xb7,
                        &[
                            uint(0xf7, cue.track),
                            uint(0xf1, cluster_position),
                            uint(0xf0, relative),
                        ]
                        .concat(),
                    );
                    cues.extend(encoded(0xbb, &[uint(0xb3, cue_time), location].concat()));
                }
                copy_range(input, output, item.start, item.end)?;
                relative += item.end - item.start;
                progress(io(output.stream_position())?);
            }
        }
        offset_ms += durations_ms[index];
    }
    if cues.is_empty() {
        return Err(INVALID.into());
    }
    let cues_position = io(output.stream_position())? - base;
    io(output.write_all(&encoded(CUES, &cues)))?;
    let end = io(output.stream_position())?;
    io(output.seek(SeekFrom::Start(base)))?;
    io(output.write_all(&seek_head(info_position, tracks_position, cues_position)))?;
    io(output.seek(SeekFrom::Start(size_position)))?;
    io(output.write_all(&size(end - base)))?;
    io(output.seek(SeekFrom::Start(end)))?;
    progress(end);
    Ok(())
}

/// Verifies structure, duration, cues and every encoded block before any source
/// is deleted. A digest over block elements proves that remuxing kept all media.
#[derive(Debug, PartialEq, Eq)]
pub struct MediaIdentity {
    pub blocks: u64,
    pub digest: [u8; 32],
}
pub fn media_identity<R: Read + Seek>(inputs: &mut [R]) -> Result<MediaIdentity, String> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    let mut blocks = 0;
    let mut buffer = [0_u8; 64 * 1024];
    for input in inputs {
        let parsed = layout(input)?;
        for cluster in parsed.elements.iter().filter(|item| item.id == CLUSTER) {
            for field in children(input, cluster.data, cluster.end)? {
                if !matches!(field.id, 0xa3 | 0xa0) {
                    continue;
                }
                io(input.seek(SeekFrom::Start(field.start)))?;
                let mut remaining = field.end - field.start;
                while remaining > 0 {
                    let count = remaining.min(buffer.len() as u64) as usize;
                    io(input.read_exact(&mut buffer[..count]))?;
                    hash.update(&buffer[..count]);
                    remaining -= count as u64;
                }
                blocks += 1;
            }
        }
    }
    if blocks == 0 {
        return Err(INVALID.into());
    }
    Ok(MediaIdentity {
        blocks,
        digest: hash.finalize().into(),
    })
}
pub fn verify_webm<R: Read + Seek>(input: &mut R, duration_ms: f64) -> Result<(), String> {
    let parsed = layout(input)?;
    if parsed.segment.unknown || parsed.segment.end != io(input.seek(SeekFrom::End(0)))? {
        return Err(INVALID.into());
    }
    let duration = children(input, parsed.info.data, parsed.info.end)?
        .into_iter()
        .find(|item| item.id == 0x4489)
        .ok_or(INVALID)?;
    let bytes: [u8; 8] = payload(input, duration)?.try_into().map_err(|_| INVALID)?;
    let actual = f64::from_be_bytes(bytes) * parsed.scale as f64 / 1_000_000.0;
    if !actual.is_finite() || (actual - duration_ms).abs() > 1.0 {
        return Err("recording_verification_failed".into());
    }
    let cues = *parsed
        .elements
        .iter()
        .find(|item| item.id == CUES)
        .ok_or(INVALID)?;
    let points = children(input, cues.data, cues.end)?;
    if points.is_empty() {
        return Err(INVALID.into());
    }
    let mut expected = Vec::new();
    for cluster in parsed.elements.iter().filter(|v| v.id == CLUSTER) {
        for cue in cluster_cues(input, *cluster, parsed.track)? {
            expected.push((
                cue.time,
                cue.track,
                cluster.start - parsed.segment.data,
                cue.relative,
            ));
        }
    }
    if points.len() != expected.len() {
        return Err("recording_verification_failed".into());
    }
    let mut previous_time = 0;
    for (point, expected) in points.into_iter().zip(expected) {
        let fields = children(input, point.data, point.end)?;
        let time = unsigned(input, *fields.iter().find(|v| v.id == 0xb3).ok_or(INVALID)?)?;
        if time < previous_time {
            return Err(INVALID.into());
        }
        previous_time = time;
        let location = *fields.iter().find(|v| v.id == 0xb7).ok_or(INVALID)?;
        let fields = children(input, location.data, location.end)?;
        let position = unsigned(input, *fields.iter().find(|v| v.id == 0xf1).ok_or(INVALID)?)?;
        let relative = unsigned(input, *fields.iter().find(|v| v.id == 0xf0).ok_or(INVALID)?)?;
        let track = unsigned(input, *fields.iter().find(|v| v.id == 0xf7).ok_or(INVALID)?)?;
        if (time, track, position, relative) != expected {
            return Err("recording_verification_failed".into());
        }
        let cluster = parsed
            .elements
            .iter()
            .find(|v| v.id == CLUSTER && v.start == parsed.segment.data + position)
            .ok_or(INVALID)?;
        let block = element(input, cluster.data + relative, cluster.end)?;
        if !matches!(block.id, 0xa3 | 0xa0) {
            return Err(INVALID.into());
        }
    }
    let head = *parsed
        .elements
        .iter()
        .find(|v| v.id == SEEK_HEAD)
        .ok_or(INVALID)?;
    let entries = children(input, head.data, head.end)?;
    for target in [parsed.info, parsed.tracks, cues] {
        let mut found = false;
        for entry in &entries {
            let fields = children(input, entry.data, entry.end)?;
            let id = *fields.iter().find(|v| v.id == 0x53ab).ok_or(INVALID)?;
            let position = unsigned(
                input,
                *fields.iter().find(|v| v.id == 0x53ac).ok_or(INVALID)?,
            )?;
            if payload(input, id)? == id_bytes(target.id)
                && position == target.start - parsed.segment.data
            {
                found = true;
            }
        }
        if !found {
            return Err("recording_verification_failed".into());
        }
    }
    Ok(())
}

#[derive(Default)]
pub struct StreamScanner {
    position: u64,
    nested: u64,
    cluster: Option<Element>,
    pub prefix: u64,
    pub scale: u64,
    pub overhead: u64,
    track: u64,
}
pub struct ClusterBoundary {
    pub start: u64,
    pub time_ms: f64,
    pub keyframe: bool,
}

/// Reads only complete EBML headers; incomplete writes wait for the next append.
fn partial_element<R: Read + Seek>(
    input: &mut R,
    start: u64,
    end: u64,
) -> Result<Option<Element>, String> {
    if start >= end {
        return Ok(None);
    }
    io(input.seek(SeekFrom::Start(start)))?;
    let result = (|| {
        let (id, _, _) = vint(input, true)?;
        let (length, _, unknown) = vint(input, false)?;
        let data = io(input.stream_position())?;
        let stop = if unknown {
            end
        } else {
            data.checked_add(length).ok_or(INVALID)?
        };
        Ok(Element {
            id: id as u32,
            start,
            data,
            end: stop,
            unknown,
        })
    })();
    match result {
        Err(error) if error == "recording_io_failed" => Ok(None),
        other => other.map(Some),
    }
}
impl StreamScanner {
    pub fn scan<R: Read + Seek>(&mut self, input: &mut R) -> Result<Vec<ClusterBoundary>, String> {
        let end = io(input.seek(SeekFrom::End(0)))?;
        let mut completed = Vec::new();
        if self.position == 0 {
            let Some(header) = partial_element(input, 0, end)? else {
                return Ok(completed);
            };
            if header.end > end {
                return Ok(completed);
            }
            if header.id != 0x1a45dfa3 || header.unknown {
                return Err(INVALID.into());
            }
            let Some(segment) = partial_element(input, header.end, end)? else {
                return Ok(completed);
            };
            if segment.id != SEGMENT {
                return Err(INVALID.into());
            }
            self.position = segment.data;
            self.scale = 1_000_000;
            self.track = 1;
        }
        loop {
            if let Some(mut cluster) = self.cluster {
                loop {
                    if !cluster.unknown && self.nested == cluster.end {
                        break;
                    }
                    let Some(child) = partial_element(input, self.nested, end)? else {
                        return Ok(completed);
                    };
                    if cluster.unknown && is_level_one(child.id) {
                        cluster.end = child.start;
                        break;
                    }
                    if child.unknown {
                        return Err(INVALID.into());
                    }
                    if !cluster.unknown && child.end > cluster.end {
                        return Err(INVALID.into());
                    }
                    if child.end > end {
                        return Ok(completed);
                    }
                    if matches!(child.id, 0xa3 | 0xa0) {
                        self.overhead = self.overhead.saturating_add(96);
                    }
                    self.nested = child.end;
                }
                let fields = children(input, cluster.data, cluster.end)?;
                let time = match fields.iter().find(|v| v.id == 0xe7) {
                    Some(v) => unsigned(input, *v)?,
                    None => 0,
                };
                let first_block = fields.iter().find(|v| matches!(v.id, 0xa3 | 0xa0));
                let cues = cluster_cues(input, cluster, self.track)?;
                let keyframe = first_block.is_some_and(|block| {
                    cues.iter()
                        .any(|cue| cue.relative == block.start - cluster.data)
                });
                completed.push(ClusterBoundary {
                    start: cluster.start,
                    time_ms: time as f64 * self.scale as f64 / 1_000_000.0,
                    keyframe,
                });
                self.cluster = None;
                self.position = cluster.end;
            }
            let Some(item) = partial_element(input, self.position, end)? else {
                break;
            };
            if item.id == CLUSTER {
                if self.prefix == 0 {
                    self.prefix = item.start;
                }
                self.overhead = self.overhead.saturating_add(32);
                self.cluster = Some(item);
                self.nested = item.data;
            } else {
                if item.unknown {
                    return Err(INVALID.into());
                }
                if item.end > end {
                    break;
                }
                if item.id == TRACKS {
                    self.track = video_track(input, item)?;
                }
                if item.id == INFO {
                    if let Some(scale) = children(input, item.data, item.end)?
                        .iter()
                        .find(|v| v.id == 0x2ad7b1)
                    {
                        self.scale = unsigned(input, *scale)?;
                    }
                    if self.scale == 0 {
                        return Err(INVALID.into());
                    }
                }
                self.position = item.end;
            }
        }
        Ok(completed)
    }
    /// Last complete cluster boundary, for best-effort recovery of a torn tail.
    pub fn complete_end(&self) -> u64 {
        if self.cluster.is_some() {
            self.nested
        } else {
            self.position
        }
    }
    pub fn output_bound(&self, bytes: u64) -> u64 {
        bytes
            .saturating_add(self.overhead)
            .saturating_add(bytes.saturating_sub(self.complete_end()).saturating_mul(16))
            .saturating_add(512)
    }
}

/// Duration of the recoverable encoded timeline (the recorder emits 20 fps).
pub fn media_duration_ms<R: Read + Seek>(input: &mut R) -> Result<f64, String> {
    let parsed = layout(input)?;
    let mut latest = parsed.origin as i128;
    for cluster in parsed.elements.iter().filter(|v| v.id == CLUSTER) {
        let fields = children(input, cluster.data, cluster.end)?;
        let timestamp = match fields.iter().find(|v| v.id == 0xe7) {
            Some(v) => unsigned(input, *v)?,
            None => 0,
        };
        for field in fields {
            let block = if field.id == 0xa3 {
                Some(field)
            } else if field.id == 0xa0 {
                children(input, field.data, field.end)?
                    .into_iter()
                    .find(|v| v.id == 0xa1)
            } else {
                None
            };
            let Some(block) = block else {
                continue;
            };
            io(input.seek(SeekFrom::Start(block.data)))?;
            let (track, _, _) = vint(input, false)?;
            let mut header = [0; 3];
            io(input.read_exact(&mut header))?;
            if track == parsed.track {
                latest = latest.max(
                    timestamp as i128 + i128::from(i16::from_be_bytes([header[0], header[1]])),
                );
            }
        }
    }
    Ok(
        ((latest - parsed.origin as i128) as f64 * parsed.scale as f64 / 1_000_000.0 + 50.0)
            .max(1.0),
    )
}

/// A duplicated header must not retain a finite size from the original stream.
pub fn make_streaming_header<R: Read + Write + Seek>(input: &mut R) -> Result<(), String> {
    let length = io(input.seek(SeekFrom::End(0)))?;
    let header = element(input, 0, length)?;
    io(input.seek(SeekFrom::Start(header.end)))?;
    let (id, _, _) = vint(input, true)?;
    if id != u64::from(SEGMENT) {
        return Err(INVALID.into());
    }
    let position = io(input.stream_position())?;
    let (_, count, _) = vint(input, false)?;
    let mut bytes = vec![0xff; count];
    bytes[0] = (0xff_u16 >> (count - 1)) as u8;
    io(input.seek(SeekFrom::Start(position)))?;
    io(input.write_all(&bytes))
}
