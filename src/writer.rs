use std::fs;
use std::path::Path;

use concord::{Enu, to_wgs_from_enu};

use crate::error::{Error, Result};
use crate::types::{GridData, Layer, RasterCollection, SampleFormat};

const TIFF_TYPE_ASCII: u16 = 2;
const TIFF_TYPE_SHORT: u16 = 3;
const TIFF_TYPE_LONG: u16 = 4;
const TIFF_TYPE_DOUBLE: u16 = 12;

const TAG_IMAGE_WIDTH: u16 = 256;
const TAG_IMAGE_LENGTH: u16 = 257;
const TAG_BITS_PER_SAMPLE: u16 = 258;
const TAG_COMPRESSION: u16 = 259;
const TAG_PHOTOMETRIC: u16 = 262;
const TAG_IMAGE_DESCRIPTION: u16 = 270;
const TAG_STRIP_OFFSETS: u16 = 273;
const TAG_SAMPLES_PER_PIXEL: u16 = 277;
const TAG_ROWS_PER_STRIP: u16 = 278;
const TAG_STRIP_BYTE_COUNTS: u16 = 279;
const TAG_PLANAR_CONFIG: u16 = 284;
const TAG_COLOR_MAP: u16 = 320;
const TAG_EXTRA_SAMPLES: u16 = 338;
const TAG_SAMPLE_FORMAT: u16 = 339;

const TAG_MODEL_PIXEL_SCALE: u16 = 33_550;
const TAG_MODEL_TIEPOINT: u16 = 33_922;
const TAG_MODEL_TRANSFORMATION: u16 = 34_264;
const TAG_GEO_KEY_DIRECTORY: u16 = 34_735;
const TAG_GEO_DOUBLE_PARAMS: u16 = 34_736;
const TAG_GEO_ASCII_PARAMS: u16 = 34_737;
const TAG_GDAL_NODATA: u16 = 42_113;

const ROTATION_THRESHOLD_RAD: f64 = 0.000_2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOptions {
    pub software: String,
    /// Rows per strip. 0 = auto (target ~8KB strips), u32::MAX = single strip.
    pub rows_per_strip: u32,
    pub force_bigtiff: bool,
    pub big_endian: bool,
    /// 1 = chunky (interleaved samples), 2 = separated planes.
    pub planar_config: u16,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            software: "rastera 0.0.1".to_owned(),
            rows_per_strip: 0,
            force_bigtiff: false,
            big_endian: false,
            planar_config: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ByteOrder {
    little_endian: bool,
}

impl ByteOrder {
    fn u16(self, v: u16) -> [u8; 2] {
        if self.little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    fn u32(self, v: u32) -> [u8; 4] {
        if self.little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    fn u64(self, v: u64) -> [u8; 8] {
        if self.little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    fn i16(self, v: i16) -> [u8; 2] {
        if self.little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    fn i32(self, v: i32) -> [u8; 4] {
        if self.little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    fn f32(self, v: f32) -> [u8; 4] {
        if self.little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    fn f64(self, v: f64) -> [u8; 8] {
        if self.little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct IfdEntry {
    tag: u16,
    field_type: u16,
    count: u64,
    value_or_offset: u64,
}

struct Payload {
    tag: u16,
    bytes: Vec<u8>,
}

struct EncodedStrip {
    bytes: Vec<u8>,
}

struct EncodedLayer {
    entries: Vec<IfdEntry>,
    payloads: Vec<Payload>,
    strips: Vec<EncodedStrip>,
    strip_offsets_array: Option<Vec<u8>>,
    strip_byte_counts_array: Option<Vec<u8>>,
}

pub fn to_tiff_bytes(collection: &RasterCollection, options: &WriteOptions) -> Result<Vec<u8>> {
    if collection.layers.is_empty() {
        return Err(Error::Message("toTiffBytes(): no layers".to_owned()));
    }

    let bo = ByteOrder {
        little_endian: !options.big_endian,
    };

    let mut encoded_layers = Vec::with_capacity(collection.layers.len());

    for layer in &collection.layers {
        encoded_layers.push(encode_layer(layer, collection, options, bo)?);
    }

    let total_strip_bytes: u64 = encoded_layers
        .iter()
        .flat_map(|l| l.strips.iter().map(|s| s.bytes.len() as u64))
        .sum();
    let use_bigtiff = options.force_bigtiff || total_strip_bytes + 1_000_000 > u64::from(u32::MAX);

    if use_bigtiff {
        write_bigtiff(&mut encoded_layers, bo)
    } else {
        write_classic_tiff(&mut encoded_layers, bo)
    }
}

fn encode_layer(
    layer: &Layer,
    collection: &RasterCollection,
    options: &WriteOptions,
    bo: ByteOrder,
) -> Result<EncodedLayer> {
    let (width, height, bits_per_sample, samples_per_pixel, sample_format, bytes_per_pixel) =
        grid_shape(&layer.grid);
    if width == 0 || height == 0 {
        return Err(Error::Validation {
            field: "grid dimensions",
            expected: "non-zero width and height".to_owned(),
            actual: format!("{width}x{height}"),
        });
    }

    let rows_per_strip = compute_rows_per_strip(
        options.rows_per_strip,
        height,
        width,
        bytes_per_pixel as u32,
    );
    let num_strips = height.div_ceil(rows_per_strip);

    let planar_config = if options.planar_config == 2 { 2u16 } else { 1u16 };
    let num_planes = if planar_config == 2 {
        samples_per_pixel as u32
    } else {
        1
    };
    let mut strips = Vec::with_capacity((num_strips * num_planes) as usize);
    for plane in 0..num_planes {
        for strip_idx in 0..num_strips {
            let start_row = strip_idx * rows_per_strip;
            let end_row = ((strip_idx + 1) * rows_per_strip).min(height);
            let bytes = encode_strip_bytes(
                &layer.grid,
                start_row,
                end_row,
                bo,
                planar_config,
                plane,
                samples_per_pixel,
            )?;
            strips.push(EncodedStrip { bytes });
        }
    }

    let mut description_bytes = build_image_description(collection, layer).into_bytes();
    description_bytes.push(0);
    let description_len = description_bytes.len() as u64;

    let mut payloads = vec![Payload {
        tag: TAG_IMAGE_DESCRIPTION,
        bytes: description_bytes,
    }];

    let is_rotated = has_rotation(layer);
    let geo_tags = build_geotiff_tags(layer, is_rotated, bo)?;

    if is_rotated {
        payloads.push(Payload {
            tag: TAG_MODEL_TRANSFORMATION,
            bytes: geo_tags.transform_bytes.expect("set when rotated"),
        });
    } else {
        payloads.push(Payload {
            tag: TAG_MODEL_PIXEL_SCALE,
            bytes: geo_tags.pixel_scale_bytes.expect("set when un-rotated"),
        });
        payloads.push(Payload {
            tag: TAG_MODEL_TIEPOINT,
            bytes: geo_tags.tiepoint_bytes.expect("set when un-rotated"),
        });
    }
    payloads.push(Payload {
        tag: TAG_GEO_KEY_DIRECTORY,
        bytes: geo_tags.geo_key_directory_bytes,
    });
    if let Some(bytes) = geo_tags.geo_double_params_bytes {
        payloads.push(Payload {
            tag: TAG_GEO_DOUBLE_PARAMS,
            bytes,
        });
    }
    if let Some(bytes) = geo_tags.geo_ascii_params_bytes {
        payloads.push(Payload {
            tag: TAG_GEO_ASCII_PARAMS,
            bytes,
        });
    }
    if let Some(bytes) = geo_tags.nodata_bytes {
        payloads.push(Payload {
            tag: TAG_GDAL_NODATA,
            bytes,
        });
    }

    let mut entries = vec![
        IfdEntry {
            tag: TAG_IMAGE_WIDTH,
            field_type: TIFF_TYPE_LONG,
            count: 1,
            value_or_offset: u64::from(width),
        },
        IfdEntry {
            tag: TAG_IMAGE_LENGTH,
            field_type: TIFF_TYPE_LONG,
            count: 1,
            value_or_offset: u64::from(height),
        },
        IfdEntry {
            tag: TAG_BITS_PER_SAMPLE,
            field_type: TIFF_TYPE_SHORT,
            count: 1,
            value_or_offset: u64::from(bits_per_sample),
        },
        short_entry(TAG_COMPRESSION, 1),
        short_entry(
            TAG_PHOTOMETRIC,
            photometric_value(&layer.grid, samples_per_pixel, layer.palette.is_some()),
        ),
        IfdEntry {
            tag: TAG_IMAGE_DESCRIPTION,
            field_type: TIFF_TYPE_ASCII,
            count: description_len,
            value_or_offset: 0,
        },
        IfdEntry {
            tag: TAG_STRIP_OFFSETS,
            field_type: TIFF_TYPE_LONG,
            count: (num_strips as u64) * (num_planes as u64),
            value_or_offset: 0,
        },
        IfdEntry {
            tag: TAG_SAMPLES_PER_PIXEL,
            field_type: TIFF_TYPE_SHORT,
            count: 1,
            value_or_offset: u64::from(samples_per_pixel),
        },
        IfdEntry {
            tag: TAG_ROWS_PER_STRIP,
            field_type: TIFF_TYPE_LONG,
            count: 1,
            value_or_offset: u64::from(rows_per_strip),
        },
        IfdEntry {
            tag: TAG_STRIP_BYTE_COUNTS,
            field_type: TIFF_TYPE_LONG,
            count: (num_strips as u64) * (num_planes as u64),
            value_or_offset: 0,
        },
        short_entry(TAG_PLANAR_CONFIG, planar_config),
        short_entry(TAG_SAMPLE_FORMAT, sample_format as u16),
    ];

    if samples_per_pixel > 3 {
        entries.push(short_entry(TAG_EXTRA_SAMPLES, 2));
    }

    if is_rotated {
        entries.push(IfdEntry {
            tag: TAG_MODEL_TRANSFORMATION,
            field_type: TIFF_TYPE_DOUBLE,
            count: 16,
            value_or_offset: 0,
        });
    } else {
        entries.push(IfdEntry {
            tag: TAG_MODEL_PIXEL_SCALE,
            field_type: TIFF_TYPE_DOUBLE,
            count: 3,
            value_or_offset: 0,
        });
        entries.push(IfdEntry {
            tag: TAG_MODEL_TIEPOINT,
            field_type: TIFF_TYPE_DOUBLE,
            count: 6,
            value_or_offset: 0,
        });
    }
    entries.push(IfdEntry {
        tag: TAG_GEO_KEY_DIRECTORY,
        field_type: TIFF_TYPE_SHORT,
        count: u64::from(geo_tags.geo_key_directory_count),
        value_or_offset: 0,
    });

    if let Some(bytes) = payloads
        .iter()
        .find(|p| p.tag == TAG_GEO_DOUBLE_PARAMS)
        .map(|p| p.bytes.as_slice())
    {
        entries.push(IfdEntry {
            tag: TAG_GEO_DOUBLE_PARAMS,
            field_type: TIFF_TYPE_DOUBLE,
            count: (bytes.len() / 8) as u64,
            value_or_offset: 0,
        });
    }
    if let Some(bytes) = payloads
        .iter()
        .find(|p| p.tag == TAG_GEO_ASCII_PARAMS)
        .map(|p| p.bytes.as_slice())
    {
        entries.push(IfdEntry {
            tag: TAG_GEO_ASCII_PARAMS,
            field_type: TIFF_TYPE_ASCII,
            count: bytes.len() as u64,
            value_or_offset: 0,
        });
    }
    if let Some(bytes) = payloads
        .iter()
        .find(|p| p.tag == TAG_GDAL_NODATA)
        .map(|p| p.bytes.as_slice())
    {
        entries.push(IfdEntry {
            tag: TAG_GDAL_NODATA,
            field_type: TIFF_TYPE_ASCII,
            count: bytes.len() as u64,
            value_or_offset: 0,
        });
    }

    if let Some(palette) = layer.palette.as_ref() {
        let required_len = 1usize << bits_per_sample;
        if palette.len() != required_len {
            return Err(Error::Validation {
                field: "palette length",
                expected: format!("{required_len} entries"),
                actual: palette.len().to_string(),
            });
        }
        let mut bytes = Vec::with_capacity(required_len * 6);
        for (r, _, _) in palette {
            bytes.extend_from_slice(&bo.u16(*r));
        }
        for (_, g, _) in palette {
            bytes.extend_from_slice(&bo.u16(*g));
        }
        for (_, _, b) in palette {
            bytes.extend_from_slice(&bo.u16(*b));
        }
        payloads.push(Payload {
            tag: TAG_COLOR_MAP,
            bytes,
        });
        entries.push(IfdEntry {
            tag: TAG_COLOR_MAP,
            field_type: TIFF_TYPE_SHORT,
            count: (required_len * 3) as u64,
            value_or_offset: 0,
        });
    }

    for (tag, values) in &layer.custom_tags {
        let count = values.len() as u64;
        if values.len() == 1 {
            entries.push(IfdEntry {
                tag: *tag,
                field_type: TIFF_TYPE_LONG,
                count: 1,
                value_or_offset: u64::from(values[0]),
            });
        } else {
            let bytes = values
                .iter()
                .flat_map(|v| bo.u32(*v))
                .collect::<Vec<_>>();
            payloads.push(Payload { tag: *tag, bytes });
            entries.push(IfdEntry {
                tag: *tag,
                field_type: TIFF_TYPE_LONG,
                count,
                value_or_offset: 0,
            });
        }
    }

    entries.sort_by_key(|e| e.tag);

    Ok(EncodedLayer {
        entries,
        payloads,
        strips,
        strip_offsets_array: None,
        strip_byte_counts_array: None,
    })
}

fn write_classic_tiff(encoded_layers: &mut [EncodedLayer], bo: ByteOrder) -> Result<Vec<u8>> {
    let mut ifd_sizes = Vec::with_capacity(encoded_layers.len());
    for layer in encoded_layers.iter() {
        let count = layer.entries.len() as u32;
        ifd_sizes.push(2u64 + u64::from(count) * 12 + 4);
    }

    let mut out = Vec::new();
    out.extend_from_slice(if bo.little_endian { b"II" } else { b"MM" });
    out.extend_from_slice(&bo.u16(42));
    out.extend_from_slice(&bo.u32(8));

    let mut payload_offset = 8u64 + ifd_sizes.iter().sum::<u64>();
    let mut current_ifd_offset = 8u64;
    let total_layers = encoded_layers.len();

    for (index, layer) in encoded_layers.iter_mut().enumerate() {
        assign_payload_offsets_and_strips(layer, &mut payload_offset, false, bo)?;
        if payload_offset > u64::from(u32::MAX) {
            return Err(Error::Message(
                "classic TIFF file would exceed 4GiB".to_owned(),
            ));
        }

        let entry_count = u16::try_from(layer.entries.len()).map_err(|_| Error::Validation {
            field: "IFD entry count",
            expected: "value that fits in u16".to_owned(),
            actual: layer.entries.len().to_string(),
        })?;
        out.extend_from_slice(&bo.u16(entry_count));
        for entry in layer.entries.iter().copied() {
            out.extend_from_slice(&bo.u16(entry.tag));
            out.extend_from_slice(&bo.u16(entry.field_type));
            out.extend_from_slice(&bo.u32(entry.count as u32));
            let inline_bytes = encode_inline_value(&entry, bo, false);
            out.extend_from_slice(&inline_bytes);
        }

        current_ifd_offset += ifd_sizes[index];
        let next_ifd = if index + 1 < total_layers {
            current_ifd_offset as u32
        } else {
            0
        };
        out.extend_from_slice(&bo.u32(next_ifd));
    }

    append_payloads_and_strips(&mut out, encoded_layers);
    Ok(out)
}

fn write_bigtiff(encoded_layers: &mut [EncodedLayer], bo: ByteOrder) -> Result<Vec<u8>> {
    let mut ifd_sizes = Vec::with_capacity(encoded_layers.len());
    for layer in encoded_layers.iter() {
        let count = layer.entries.len() as u64;
        ifd_sizes.push(8 + count * 20 + 8);
    }

    let mut out = Vec::new();
    out.extend_from_slice(if bo.little_endian { b"II" } else { b"MM" });
    out.extend_from_slice(&bo.u16(43));
    out.extend_from_slice(&bo.u16(8));
    out.extend_from_slice(&bo.u16(0));
    out.extend_from_slice(&bo.u64(16));

    let mut payload_offset = 16u64 + ifd_sizes.iter().sum::<u64>();
    let mut current_ifd_offset = 16u64;
    let total_layers = encoded_layers.len();

    for (index, layer) in encoded_layers.iter_mut().enumerate() {
        assign_payload_offsets_and_strips(layer, &mut payload_offset, true, bo)?;

        let entry_count = layer.entries.len() as u64;
        out.extend_from_slice(&bo.u64(entry_count));
        for entry in layer.entries.iter().copied() {
            out.extend_from_slice(&bo.u16(entry.tag));
            out.extend_from_slice(&bo.u16(entry.field_type));
            out.extend_from_slice(&bo.u64(entry.count));
            let inline_bytes = encode_inline_value(&entry, bo, true);
            out.extend_from_slice(&inline_bytes);
        }

        current_ifd_offset += ifd_sizes[index];
        let next_ifd = if index + 1 < total_layers {
            current_ifd_offset
        } else {
            0
        };
        out.extend_from_slice(&bo.u64(next_ifd));
    }

    append_payloads_and_strips(&mut out, encoded_layers);
    Ok(out)
}

/// Encode the inline "value or offset" field. For small inline values (BYTE, SHORT, LONG
/// with count that fits), the value must be left-justified per TIFF 6.0 — i.e., the value's
/// bytes occupy the lower-numbered bytes of the 4- or 8-byte field, with the remaining
/// bytes as zero padding. In BE, "lower-numbered" means the most significant bytes.
fn encode_inline_value(entry: &IfdEntry, bo: ByteOrder, bigtiff: bool) -> Vec<u8> {
    let field_size = if bigtiff { 8 } else { 4 };
    let mut out = vec![0u8; field_size];
    let element_size = match entry.field_type {
        TIFF_TYPE_ASCII => 1,
        TIFF_TYPE_SHORT => 2,
        TIFF_TYPE_LONG => 4,
        16 => 8, // LONG8 (BigTIFF)
        _ => field_size, // DOUBLE / unknown — value_or_offset is always an out-of-line offset
    };
    let total = element_size * entry.count as usize;
    if total <= field_size {
        match element_size {
            2 => {
                let v = entry.value_or_offset as u16;
                let b = bo.u16(v);
                out[..2].copy_from_slice(&b);
            }
            4 => {
                let v = entry.value_or_offset as u32;
                let b = bo.u32(v);
                out[..4].copy_from_slice(&b);
            }
            8 => {
                let v = entry.value_or_offset;
                let b = bo.u64(v);
                out[..8].copy_from_slice(&b);
            }
            _ => {
                out[0] = entry.value_or_offset as u8;
            }
        }
        return out;
    }
    if bigtiff {
        out.copy_from_slice(&bo.u64(entry.value_or_offset));
    } else {
        out.copy_from_slice(&bo.u32(entry.value_or_offset as u32));
    }
    out
}

fn assign_payload_offsets_and_strips(
    layer: &mut EncodedLayer,
    payload_offset: &mut u64,
    bigtiff: bool,
    bo: ByteOrder,
) -> Result<()> {
    for payload in &layer.payloads {
        let entry = layer
            .entries
            .iter_mut()
            .find(|e| e.tag == payload.tag)
            .ok_or_else(|| Error::Message(format!("missing payload entry {}", payload.tag)))?;
        entry.value_or_offset = *payload_offset;
        *payload_offset += payload.bytes.len() as u64;
    }

    let num_strips = layer.strips.len() as u64;
    let offset_field_size: u64 = if bigtiff { 8 } else { 4 };
    let strip_byte_counts: Vec<u64> = layer.strips.iter().map(|s| s.bytes.len() as u64).collect();

    if num_strips == 1 {
        let strip_offset = *payload_offset;
        let so_entry = layer
            .entries
            .iter_mut()
            .find(|e| e.tag == TAG_STRIP_OFFSETS)
            .ok_or_else(|| Error::Message("missing StripOffsets entry".to_owned()))?;
        so_entry.value_or_offset = strip_offset;
        so_entry.field_type = TIFF_TYPE_LONG;
        let sbc_entry = layer
            .entries
            .iter_mut()
            .find(|e| e.tag == TAG_STRIP_BYTE_COUNTS)
            .ok_or_else(|| Error::Message("missing StripByteCounts entry".to_owned()))?;
        sbc_entry.value_or_offset = strip_byte_counts[0];
        sbc_entry.field_type = TIFF_TYPE_LONG;
        *payload_offset += strip_byte_counts[0];
    } else {
        let strip_offsets_array_offset = *payload_offset;
        *payload_offset += num_strips * offset_field_size;
        let strip_counts_array_offset = *payload_offset;
        *payload_offset += num_strips * offset_field_size;

        let mut strip_offsets: Vec<u64> = Vec::with_capacity(num_strips as usize);
        let mut cursor = *payload_offset;
        for bytes in &strip_byte_counts {
            strip_offsets.push(cursor);
            cursor += *bytes;
        }
        *payload_offset = cursor;

        layer.strip_offsets_array = Some(encode_long_array(&strip_offsets, bigtiff, bo));
        layer.strip_byte_counts_array = Some(encode_long_array(&strip_byte_counts, bigtiff, bo));

        let so_entry = layer
            .entries
            .iter_mut()
            .find(|e| e.tag == TAG_STRIP_OFFSETS)
            .ok_or_else(|| Error::Message("missing StripOffsets entry".to_owned()))?;
        so_entry.value_or_offset = strip_offsets_array_offset;
        so_entry.field_type = if bigtiff { 16 } else { TIFF_TYPE_LONG };
        let sbc_entry = layer
            .entries
            .iter_mut()
            .find(|e| e.tag == TAG_STRIP_BYTE_COUNTS)
            .ok_or_else(|| Error::Message("missing StripByteCounts entry".to_owned()))?;
        sbc_entry.value_or_offset = strip_counts_array_offset;
        sbc_entry.field_type = if bigtiff { 16 } else { TIFF_TYPE_LONG };
    }

    Ok(())
}

fn encode_long_array(values: &[u64], bigtiff: bool, bo: ByteOrder) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * if bigtiff { 8 } else { 4 });
    for v in values {
        if bigtiff {
            out.extend_from_slice(&bo.u64(*v));
        } else {
            out.extend_from_slice(&bo.u32(*v as u32));
        }
    }
    out
}

fn append_payloads_and_strips(out: &mut Vec<u8>, encoded_layers: &mut [EncodedLayer]) {
    for layer in encoded_layers.iter() {
        for payload in &layer.payloads {
            out.extend_from_slice(&payload.bytes);
        }
        if let Some(bytes) = layer.strip_offsets_array.as_ref() {
            out.extend_from_slice(bytes);
        }
        if let Some(bytes) = layer.strip_byte_counts_array.as_ref() {
            out.extend_from_slice(bytes);
        }
        for strip in &layer.strips {
            out.extend_from_slice(&strip.bytes);
        }
    }
}

pub fn write_raster_collection(
    collection: &RasterCollection,
    path: impl AsRef<Path>,
    options: &WriteOptions,
) -> Result<()> {
    let bytes = to_tiff_bytes(collection, options)?;
    fs::write(path.as_ref(), bytes).map_err(|err| Error::File {
        path: path.as_ref().display().to_string(),
        operation: "write",
        reason: err.to_string(),
    })
}

#[allow(non_snake_case)]
pub fn toTiffBytes(collection: &RasterCollection, options: &WriteOptions) -> Result<Vec<u8>> {
    to_tiff_bytes(collection, options)
}

#[allow(non_snake_case)]
pub fn WriteRasterCollection(
    collection: &RasterCollection,
    path: impl AsRef<Path>,
    options: &WriteOptions,
) -> Result<()> {
    write_raster_collection(collection, path, options)
}

fn short_entry(tag: u16, value: u16) -> IfdEntry {
    IfdEntry {
        tag,
        field_type: TIFF_TYPE_SHORT,
        count: 1,
        value_or_offset: u64::from(value),
    }
}

fn grid_shape(grid: &GridData) -> (u32, u32, u16, u16, SampleFormat, usize) {
    let (rows, cols) = grid.dimensions();
    let bits = grid.bits_per_sample();
    let spp = grid.samples_per_pixel();
    let fmt = grid.sample_format();
    let bytes_per_pixel = match grid {
        GridData::U8(_) | GridData::I8(_) => 1,
        GridData::U16(_) | GridData::I16(_) => 2,
        GridData::U32(_) | GridData::I32(_) | GridData::F32(_) => 4,
        GridData::F64(_) => 8,
        GridData::Rgba8(_) => 4,
    };
    (cols as u32, rows as u32, bits, spp, fmt, bytes_per_pixel)
}

fn photometric_value(grid: &GridData, _samples_per_pixel: u16, has_palette: bool) -> u16 {
    if has_palette {
        return 3;
    }
    match grid {
        GridData::Rgba8(_) => 2,
        _ => 1,
    }
}

fn compute_rows_per_strip(requested: u32, height: u32, width: u32, bytes_per_pixel: u32) -> u32 {
    if requested == u32::MAX {
        return height.max(1);
    }
    if requested > 0 {
        return requested.min(height).max(1);
    }
    let target_strip_bytes: u32 = 8192;
    let bytes_per_row = (width * bytes_per_pixel).max(1);
    let auto = (target_strip_bytes / bytes_per_row).max(1);
    auto.min(height).max(1)
}

fn encode_strip_bytes(
    grid: &GridData,
    start_row: u32,
    end_row: u32,
    bo: ByteOrder,
    planar_config: u16,
    plane: u32,
    samples_per_pixel: u16,
) -> Result<Vec<u8>> {
    let start = start_row as usize;
    let end = end_row as usize;
    let cols = grid.dimensions().1;
    let mut out = Vec::new();
    match grid {
        GridData::U8(g) => {
            if bo.little_endian {
                let slice = g.data.as_slice();
                for r in start..end {
                    out.extend_from_slice(&slice[r * cols..(r + 1) * cols]);
                }
            } else {
                for r in start..end {
                    for c in 0..cols {
                        out.push(g[(r, c)]);
                    }
                }
            }
        }
        GridData::I8(g) => {
            for r in start..end {
                for c in 0..cols {
                    out.push(g[(r, c)] as u8);
                }
            }
        }
        GridData::U16(g) => {
            for r in start..end {
                for c in 0..cols {
                    out.extend_from_slice(&bo.u16(g[(r, c)]));
                }
            }
        }
        GridData::I16(g) => {
            for r in start..end {
                for c in 0..cols {
                    out.extend_from_slice(&bo.i16(g[(r, c)]));
                }
            }
        }
        GridData::U32(g) => {
            for r in start..end {
                for c in 0..cols {
                    out.extend_from_slice(&bo.u32(g[(r, c)]));
                }
            }
        }
        GridData::I32(g) => {
            for r in start..end {
                for c in 0..cols {
                    out.extend_from_slice(&bo.i32(g[(r, c)]));
                }
            }
        }
        GridData::F32(g) => {
            for r in start..end {
                for c in 0..cols {
                    out.extend_from_slice(&bo.f32(g[(r, c)]));
                }
            }
        }
        GridData::F64(g) => {
            for r in start..end {
                for c in 0..cols {
                    out.extend_from_slice(&bo.f64(g[(r, c)]));
                }
            }
        }
        GridData::Rgba8(g) => {
            if planar_config == 2 {
                for r in start..end {
                    for c in 0..cols {
                        let px = g[(r, c)];
                        let byte = match plane {
                            0 => px.r,
                            1 => px.g,
                            2 => px.b,
                            3 => px.a,
                            _ => 0,
                        };
                        out.push(byte);
                    }
                }
            } else {
                for r in start..end {
                    for c in 0..cols {
                        let px = g[(r, c)];
                        out.push(px.r);
                        out.push(px.g);
                        out.push(px.b);
                        out.push(px.a);
                    }
                }
            }
        }
    }
    let _ = samples_per_pixel;
    Ok(out)
}

fn has_rotation(layer: &Layer) -> bool {
    let euler = layer.shift.rotation.to_euler();
    euler.yaw.abs() > ROTATION_THRESHOLD_RAD
}

struct GeotiffTags {
    pixel_scale_bytes: Option<Vec<u8>>,
    tiepoint_bytes: Option<Vec<u8>>,
    transform_bytes: Option<Vec<u8>>,
    geo_key_directory_bytes: Vec<u8>,
    geo_key_directory_count: u32,
    geo_double_params_bytes: Option<Vec<u8>>,
    geo_ascii_params_bytes: Option<Vec<u8>>,
    nodata_bytes: Option<Vec<u8>>,
}

fn build_geotiff_tags(layer: &Layer, is_rotated: bool, bo: ByteOrder) -> Result<GeotiffTags> {
    let width = layer.width() as f64;
    let height = layer.height() as f64;

    let datum = layer.datum;
    let shift = layer.shift;
    let resolution = layer.resolution;

    let center_wgs = to_wgs_from_enu(Enu::new(shift.point.x, shift.point.y, shift.point.z, datum));
    let east_wgs = to_wgs_from_enu(Enu::new(
        shift.point.x + resolution,
        shift.point.y,
        shift.point.z,
        datum,
    ));
    let north_wgs = to_wgs_from_enu(Enu::new(
        shift.point.x,
        shift.point.y + resolution,
        shift.point.z,
        datum,
    ));

    let scale_x = east_wgs.longitude - center_wgs.longitude;
    let scale_y = north_wgs.latitude - center_wgs.latitude;

    let half_width_m = width * resolution / 2.0;
    let half_height_m = height * resolution / 2.0;
    let west_wgs = to_wgs_from_enu(Enu::new(
        shift.point.x - half_width_m,
        shift.point.y,
        shift.point.z,
        datum,
    ));
    let north_edge_wgs = to_wgs_from_enu(Enu::new(
        shift.point.x,
        shift.point.y + half_height_m,
        shift.point.z,
        datum,
    ));
    let top_left_lon = west_wgs.longitude;
    let top_left_lat = north_edge_wgs.latitude;

    let (pixel_scale_bytes, tiepoint_bytes, transform_bytes) = if is_rotated {
        let yaw = layer.shift.rotation.to_euler().yaw;
        let cos_yaw = yaw.cos();
        let sin_yaw = yaw.sin();
        let half_w = width / 2.0;
        let half_h = height / 2.0;
        let tl_col_offset = -half_w * cos_yaw - half_h * sin_yaw;
        let tl_row_offset = -half_w * sin_yaw + half_h * cos_yaw;
        let rotated_tl_lon = center_wgs.longitude + tl_col_offset * scale_x;
        let rotated_tl_lat = center_wgs.latitude + tl_row_offset * scale_y;
        let mut buf = Vec::with_capacity(128);
        for v in [
            scale_x * cos_yaw,
            -scale_y * sin_yaw,
            0.0,
            rotated_tl_lon,
            scale_x * sin_yaw,
            scale_y * cos_yaw,
            0.0,
            rotated_tl_lat,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ] {
            buf.extend_from_slice(&bo.f64(v));
        }
        (None, None, Some(buf))
    } else {
        let mut scale = Vec::with_capacity(24);
        scale.extend_from_slice(&bo.f64(scale_x));
        scale.extend_from_slice(&bo.f64(-scale_y));
        scale.extend_from_slice(&bo.f64(0.0));

        let mut tiepoint = Vec::with_capacity(48);
        for v in [0.0, 0.0, 0.0, top_left_lon, top_left_lat, center_wgs.altitude] {
            tiepoint.extend_from_slice(&bo.f64(v));
        }
        (Some(scale), Some(tiepoint), None)
    };

    let has_geo_ascii =
        !layer.geo_ascii_params.is_empty() || !layer.vertical_citation.is_empty();
    let has_vertical_datum = layer.vertical_datum.is_some();
    let has_vertical_units = layer.vertical_units.is_some();
    let has_vertical_citation = !layer.vertical_citation.is_empty();

    let mut num_keys: u16 = 4;
    if !layer.geo_ascii_params.is_empty() {
        num_keys += 2;
    }
    if has_vertical_citation {
        num_keys += 1;
    }
    if has_vertical_datum {
        num_keys += 1;
    }
    if has_vertical_units {
        num_keys += 1;
    }

    let total_shorts = 4 + 4 * u32::from(num_keys);
    let mut shorts: Vec<u16> = Vec::with_capacity(total_shorts as usize);
    shorts.extend_from_slice(&[1, 1, 0, num_keys]);
    shorts.extend_from_slice(&[1024, 0, 1, 2]);
    shorts.extend_from_slice(&[1025, 0, 1, 1]);

    if !layer.geo_ascii_params.is_empty() {
        shorts.extend_from_slice(&[
            1026,
            TAG_GEO_ASCII_PARAMS,
            (layer.geo_ascii_params.len() + 1) as u16,
            0,
        ]);
    }
    shorts.extend_from_slice(&[2048, 0, 1, 4326]);
    if !layer.geo_ascii_params.is_empty() {
        shorts.extend_from_slice(&[
            2049,
            TAG_GEO_ASCII_PARAMS,
            (layer.geo_ascii_params.len() + 1) as u16,
            0,
        ]);
    }
    shorts.extend_from_slice(&[2054, 0, 1, 9102]);

    if has_vertical_citation {
        let offset = if !layer.geo_ascii_params.is_empty() {
            (layer.geo_ascii_params.len() + 1) as u16
        } else {
            0
        };
        shorts.extend_from_slice(&[
            4097,
            TAG_GEO_ASCII_PARAMS,
            (layer.vertical_citation.len() + 1) as u16,
            offset,
        ]);
    }
    if let Some(value) = layer.vertical_datum {
        shorts.extend_from_slice(&[4098, 0, 1, value]);
    }
    if let Some(value) = layer.vertical_units {
        shorts.extend_from_slice(&[4099, 0, 1, value]);
    }

    let mut geo_key_directory_bytes = Vec::with_capacity(shorts.len() * 2);
    for v in &shorts {
        geo_key_directory_bytes.extend_from_slice(&bo.u16(*v));
    }

    let geo_double_params_bytes = if layer.geo_double_params.is_empty() {
        None
    } else {
        let mut bytes = Vec::with_capacity(layer.geo_double_params.len() * 8);
        for v in &layer.geo_double_params {
            bytes.extend_from_slice(&bo.f64(*v));
        }
        Some(bytes)
    };

    let geo_ascii_params_bytes = if has_geo_ascii {
        let mut bytes = Vec::new();
        if !layer.geo_ascii_params.is_empty() {
            bytes.extend_from_slice(layer.geo_ascii_params.as_bytes());
            bytes.push(0);
        }
        if !layer.vertical_citation.is_empty() {
            bytes.extend_from_slice(layer.vertical_citation.as_bytes());
            bytes.push(0);
        }
        Some(bytes)
    } else {
        None
    };

    let nodata_bytes = layer.no_data_value.map(|v| {
        let mut s = format!("{v}");
        s.push('\0');
        s.into_bytes()
    });

    Ok(GeotiffTags {
        pixel_scale_bytes,
        tiepoint_bytes,
        transform_bytes,
        geo_key_directory_bytes,
        geo_key_directory_count: total_shorts,
        geo_double_params_bytes,
        geo_ascii_params_bytes,
        nodata_bytes,
    })
}

fn build_image_description(collection: &RasterCollection, layer: &Layer) -> String {
    let mut description = format!(
        concat!(
            "rastera;",
            "layer_resolution={};",
            "layer_datum_lat={};layer_datum_lon={};layer_datum_alt={};",
            "shift_x={};shift_y={};shift_z={};",
            "rot_w={};rot_x={};rot_y={};rot_z={};",
            "collection_resolution={};",
            "collection_datum_lat={};collection_datum_lon={};collection_datum_alt={}"
        ),
        layer.resolution,
        layer.datum.latitude,
        layer.datum.longitude,
        layer.datum.altitude,
        layer.shift.point.x,
        layer.shift.point.y,
        layer.shift.point.z,
        layer.shift.rotation.w,
        layer.shift.rotation.x,
        layer.shift.rotation.y,
        layer.shift.rotation.z,
        collection.resolution,
        collection.datum.latitude,
        collection.datum.longitude,
        collection.datum.altitude
    );

    let mut properties = layer
        .get_global_properties()
        .into_iter()
        .collect::<Vec<_>>();
    properties.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in properties {
        description.push(';');
        description.push_str("prop:");
        description.push_str(&escape_description_value(&key));
        description.push('=');
        description.push_str(&escape_description_value(&value));
    }

    description
}

fn escape_description_value(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(';', "%3B")
        .replace('=', "%3D")
}

#[cfg(test)]
mod tests {
    use datapod::{Geo, Grid, Pose, Quaternion, Vector};

    use super::{WriteOptions, to_tiff_bytes};
    use crate::{GridData, Layer, RasterCollection};

    fn make_grid(values: &[u8], rows: usize, cols: usize) -> Grid<u8> {
        Grid {
            rows,
            cols,
            resolution: 1.5,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(values.to_vec()),
        }
    }

    #[test]
    fn writes_minimal_tiff_header() {
        let mut layer = Layer::new(GridData::from(make_grid(&[0, 1, 2, 3, 4, 5], 2, 3)));
        layer.datum = Geo::new(1.0, 2.0, 3.0);
        layer.shift = Pose {
            point: datapod::Point::new(4.0, 5.0, 6.0),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
        };
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::new(1.0, 2.0, 3.0),
            shift: Pose::default(),
            resolution: 1.5,
        };

        let bytes = to_tiff_bytes(&collection, &WriteOptions::default()).unwrap();
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[0..2], b"II");
        assert_eq!(u16::from_le_bytes([bytes[2], bytes[3]]), 42);
    }

    #[test]
    fn writes_multi_ifd_chain() {
        let layer1 = Layer::new(GridData::from(make_grid(&[1, 2, 3, 4], 2, 2)));
        let layer2 = Layer::new(GridData::from(make_grid(&[5, 6, 7, 8], 2, 2)));
        let collection = RasterCollection {
            layers: vec![layer1, layer2],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let bytes = to_tiff_bytes(&collection, &WriteOptions::default()).unwrap();
        let first_ifd = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let first_entries = u16::from_le_bytes([bytes[first_ifd], bytes[first_ifd + 1]]) as usize;
        let next_offset_pos = first_ifd + 2 + first_entries * 12;
        let next_ifd = u32::from_le_bytes([
            bytes[next_offset_pos],
            bytes[next_offset_pos + 1],
            bytes[next_offset_pos + 2],
            bytes[next_offset_pos + 3],
        ]);
        assert!(next_ifd > 0);
    }

    #[test]
    fn emits_custom_tag_entries_in_sorted_order() {
        let mut layer = Layer::new(GridData::from(make_grid(&[1, 2, 3, 4], 2, 2)));
        layer.custom_tags.insert(50001, vec![9]);
        layer.custom_tags.insert(300, vec![7]);
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let bytes = to_tiff_bytes(&collection, &WriteOptions::default()).unwrap();
        let ifd = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let count = u16::from_le_bytes([bytes[ifd], bytes[ifd + 1]]) as usize;
        let mut prev = 0u16;
        for i in 0..count {
            let off = ifd + 2 + i * 12;
            let tag = u16::from_le_bytes([bytes[off], bytes[off + 1]]);
            assert!(tag > prev);
            prev = tag;
        }
    }
}
