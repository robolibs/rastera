use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use datapod::{Geo, Grid, Point, Pose, Quaternion, Vector};

use crate::color::Rgba8;
use crate::error::{Error, Result};
use crate::tags::PRIVATE_TAG_MIN;
use crate::types::{GridData, Layer, RasterCollection, SampleFormat};

const TIFF_TYPE_BYTE: u16 = 1;
const TIFF_TYPE_ASCII: u16 = 2;
const TIFF_TYPE_SHORT: u16 = 3;
const TIFF_TYPE_LONG: u16 = 4;
const TIFF_TYPE_DOUBLE: u16 = 12;
const TIFF_TYPE_LONG8: u16 = 16;

const TAG_IMAGE_WIDTH: u16 = 256;
const TAG_IMAGE_LENGTH: u16 = 257;
const TAG_BITS_PER_SAMPLE: u16 = 258;
const TAG_COMPRESSION: u16 = 259;
const TAG_PHOTOMETRIC: u16 = 262;
const TAG_IMAGE_DESCRIPTION: u16 = 270;
const TAG_STRIP_OFFSETS: u16 = 273;
const TAG_SAMPLES_PER_PIXEL: u16 = 277;
const TAG_STRIP_BYTE_COUNTS: u16 = 279;
const TAG_PLANAR_CONFIG: u16 = 284;
const TAG_SAMPLE_FORMAT: u16 = 339;
const TAG_COLOR_MAP: u16 = 320;
const TAG_MODEL_PIXEL_SCALE: u16 = 33_550;
const TAG_MODEL_TRANSFORMATION: u16 = 34_264;
const TAG_GEO_KEY_DIRECTORY: u16 = 34_735;
const TAG_GEO_DOUBLE_PARAMS: u16 = 34_736;
const TAG_GEO_ASCII_PARAMS: u16 = 34_737;
const TAG_GDAL_NODATA: u16 = 42_113;

#[derive(Debug, Clone, Copy)]
struct Entry {
    field_type: u16,
    count: u64,
    /// Endian-decoded offset if the data is out-of-line. When the data is
    /// inline (fits in 4 or 8 bytes), use `inline_bytes` instead — the raw
    /// bytes are left-justified per TIFF 6.0, so reading byte positions 0..N
    /// in the file's byte order yields the actual values regardless of LE/BE.
    value_or_offset: u64,
    inline_bytes: [u8; 8],
    inline_capacity: usize,
}

impl Entry {
    fn read_inline_u16(&self, header: &Header, index: usize) -> u16 {
        let lo = index * 2;
        let chunk = [self.inline_bytes[lo], self.inline_bytes[lo + 1]];
        if header.little_endian {
            u16::from_le_bytes(chunk)
        } else {
            u16::from_be_bytes(chunk)
        }
    }
    fn read_inline_u32(&self, header: &Header, index: usize) -> u32 {
        let lo = index * 4;
        let chunk = [
            self.inline_bytes[lo],
            self.inline_bytes[lo + 1],
            self.inline_bytes[lo + 2],
            self.inline_bytes[lo + 3],
        ];
        if header.little_endian {
            u32::from_le_bytes(chunk)
        } else {
            u32::from_be_bytes(chunk)
        }
    }
    fn read_inline_u64(&self, header: &Header) -> u64 {
        if header.little_endian {
            u64::from_le_bytes(self.inline_bytes)
        } else {
            u64::from_be_bytes(self.inline_bytes)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Header {
    little_endian: bool,
    bigtiff: bool,
}

impl Header {
    fn read_u16(&self, bytes: &[u8], offset: usize) -> Result<u16> {
        let slice = bytes
            .get(offset..offset + 2)
            .ok_or_else(|| Error::Message("Unexpected end of file".to_owned()))?;
        Ok(if self.little_endian {
            u16::from_le_bytes([slice[0], slice[1]])
        } else {
            u16::from_be_bytes([slice[0], slice[1]])
        })
    }
    fn read_u32(&self, bytes: &[u8], offset: usize) -> Result<u32> {
        let slice = bytes
            .get(offset..offset + 4)
            .ok_or_else(|| Error::Message("Unexpected end of file".to_owned()))?;
        Ok(if self.little_endian {
            u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
        } else {
            u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
        })
    }
    fn read_u64(&self, bytes: &[u8], offset: usize) -> Result<u64> {
        let slice = bytes
            .get(offset..offset + 8)
            .ok_or_else(|| Error::Message("Unexpected end of file".to_owned()))?;
        let arr = [
            slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
        ];
        Ok(if self.little_endian {
            u64::from_le_bytes(arr)
        } else {
            u64::from_be_bytes(arr)
        })
    }
    fn read_f64(&self, bytes: &[u8], offset: usize) -> Result<f64> {
        Ok(f64::from_bits(self.read_u64(bytes, offset)?))
    }
}

pub fn read_raster_collection(path: impl AsRef<Path>) -> Result<RasterCollection> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|err| Error::File {
        path: path.display().to_string(),
        operation: "read",
        reason: err.to_string(),
    })?;
    parse_raster_collection(&bytes)
}

#[allow(non_snake_case)]
pub fn ReadRasterCollection(path: impl AsRef<Path>) -> Result<RasterCollection> {
    read_raster_collection(path)
}

fn parse_raster_collection(bytes: &[u8]) -> Result<RasterCollection> {
    if bytes.len() < 8 {
        return Err(Error::Message("TIFF file is too small".to_owned()));
    }
    let little_endian = match &bytes[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return Err(Error::Message("Bad TIFF byte-order".to_owned())),
    };
    let header = Header {
        little_endian,
        bigtiff: false,
    };
    let magic = header.read_u16(bytes, 2)?;
    let (header, mut ifd_offset) = match magic {
        42 => {
            let offset = header.read_u32(bytes, 4)? as u64;
            (header, offset)
        }
        43 => {
            let offset_size = header.read_u16(bytes, 4)?;
            let zero = header.read_u16(bytes, 6)?;
            if offset_size != 8 || zero != 0 {
                return Err(Error::Message("Invalid BigTIFF header".to_owned()));
            }
            let bigtiff_header = Header {
                little_endian,
                bigtiff: true,
            };
            let offset = bigtiff_header.read_u64(bytes, 8)?;
            (bigtiff_header, offset)
        }
        _ => {
            return Err(Error::Message(
                "Bad TIFF magic (expected 42 or 43)".to_owned(),
            ));
        }
    };

    let mut layers = Vec::new();
    let mut collection_datum = Geo::default();
    let mut collection_shift = Pose::default();
    let mut collection_resolution = 0.0;

    while ifd_offset != 0 {
        let ifd_pos = usize::try_from(ifd_offset).map_err(|_| Error::Validation {
            field: "IFD offset",
            expected: "value that fits in usize".to_owned(),
            actual: ifd_offset.to_string(),
        })?;
        let (entries, next_ifd) = read_ifd(&header, bytes, ifd_pos)?;

        let width = read_scalar_u64(&header, &entries,TAG_IMAGE_WIDTH)?
            .ok_or_else(|| Error::Message("missing ImageWidth".to_owned()))?
            as usize;
        let height = read_scalar_u64(&header, &entries,TAG_IMAGE_LENGTH)?
            .ok_or_else(|| Error::Message("missing ImageLength".to_owned()))?
            as usize;
        if width == 0 || height == 0 {
            return Err(Error::Validation {
                field: "image dimensions",
                expected: "non-zero width and height".to_owned(),
                actual: format!("{width}x{height}"),
            });
        }

        let bits_per_sample = read_scalar_u64(&header, &entries,TAG_BITS_PER_SAMPLE)?.unwrap_or(8) as u16;
        let compression = read_scalar_u64(&header, &entries,TAG_COMPRESSION)?.unwrap_or(1) as u16;
        if compression != 1 {
            return Err(Error::Unsupported {
                feature: format!("Compression={compression}"),
            });
        }
        let samples_per_pixel = read_scalar_u64(&header, &entries,TAG_SAMPLES_PER_PIXEL)?.unwrap_or(1) as u16;
        let photometric = read_scalar_u64(&header, &entries,TAG_PHOTOMETRIC)?.unwrap_or(1) as u16;
        let sample_format_raw = read_scalar_u64(&header, &entries,TAG_SAMPLE_FORMAT)?.unwrap_or(1) as u16;
        let sample_format = match sample_format_raw {
            1 => SampleFormat::UnsignedInt,
            2 => SampleFormat::SignedInt,
            3 => SampleFormat::Float,
            4 => SampleFormat::Undefined,
            other => {
                return Err(Error::Unsupported {
                    feature: format!("SampleFormat={other}"),
                });
            }
        };
        let planar_config = read_scalar_u64(&header, &entries,TAG_PLANAR_CONFIG)?.unwrap_or(1) as u16;
        if planar_config != 1 && planar_config != 2 {
            return Err(Error::Unsupported {
                feature: format!("PlanarConfiguration={planar_config}"),
            });
        }

        let strip_offsets = read_long_array(&header, &entries, bytes, TAG_STRIP_OFFSETS)?
            .ok_or_else(|| Error::Message("missing StripOffsets".to_owned()))?;
        let strip_byte_counts = read_long_array(&header, &entries, bytes, TAG_STRIP_BYTE_COUNTS)?
            .ok_or_else(|| Error::Message("missing StripByteCounts".to_owned()))?;
        if strip_offsets.len() != strip_byte_counts.len() {
            return Err(Error::Validation {
                field: "strip arrays",
                expected: "same length".to_owned(),
                actual: format!("{} vs {}", strip_offsets.len(), strip_byte_counts.len()),
            });
        }

        let mut pixel_bytes = Vec::new();
        for (offset, count) in strip_offsets.iter().zip(strip_byte_counts.iter()) {
            let start = usize::try_from(*offset).map_err(|_| Error::Validation {
                field: "strip offset",
                expected: "value that fits in usize".to_owned(),
                actual: offset.to_string(),
            })?;
            let len = usize::try_from(*count).map_err(|_| Error::Validation {
                field: "strip byte count",
                expected: "value that fits in usize".to_owned(),
                actual: count.to_string(),
            })?;
            let end = start
                .checked_add(len)
                .ok_or_else(|| Error::Message("strip offset overflow".to_owned()))?;
            if end > bytes.len() {
                return Err(Error::Message("Strip data points outside file".to_owned()));
            }
            pixel_bytes.extend_from_slice(&bytes[start..end]);
        }

        let bytes_per_pixel = usize::from(bits_per_sample / 8) * usize::from(samples_per_pixel);
        let expected_bytes = width
            .checked_mul(height)
            .and_then(|v| v.checked_mul(bytes_per_pixel.max(1)))
            .ok_or_else(|| Error::Message("grid size overflow".to_owned()))?;
        if pixel_bytes.len() != expected_bytes {
            return Err(Error::Validation {
                field: "strip bytes",
                expected: expected_bytes.to_string(),
                actual: pixel_bytes.len().to_string(),
            });
        }

        if planar_config == 2 && samples_per_pixel > 1 {
            let spp = usize::from(samples_per_pixel);
            let bytes_per_sample = usize::from(bits_per_sample / 8).max(1);
            let pixels = width * height;
            let plane_bytes = pixels * bytes_per_sample;
            if expected_bytes != plane_bytes * spp {
                return Err(Error::Message(
                    "planar layout size mismatch".to_owned(),
                ));
            }
            let mut interleaved = vec![0u8; expected_bytes];
            for pixel in 0..pixels {
                for sample in 0..spp {
                    let src = sample * plane_bytes + pixel * bytes_per_sample;
                    let dst = pixel * spp * bytes_per_sample + sample * bytes_per_sample;
                    interleaved[dst..dst + bytes_per_sample]
                        .copy_from_slice(&pixel_bytes[src..src + bytes_per_sample]);
                }
            }
            pixel_bytes = interleaved;
        }

        let image_description = read_ascii_tag(&header, &entries, bytes, TAG_IMAGE_DESCRIPTION)?
            .unwrap_or_default();
        let metadata = ParsedMetadata::from_description(&image_description);
        let mut resolution = metadata.layer_resolution.unwrap_or(1.0);
        let mut shift = Pose {
            point: Point::new(
                metadata.shift_x.unwrap_or(0.0),
                metadata.shift_y.unwrap_or(0.0),
                metadata.shift_z.unwrap_or(0.0),
            ),
            rotation: Quaternion::new(
                metadata.rot_w.unwrap_or(1.0),
                metadata.rot_x.unwrap_or(0.0),
                metadata.rot_y.unwrap_or(0.0),
                metadata.rot_z.unwrap_or(0.0),
            ),
        };
        let layer_datum = Geo::new(
            metadata.layer_datum_lat.unwrap_or(0.0),
            metadata.layer_datum_lon.unwrap_or(0.0),
            metadata.layer_datum_alt.unwrap_or(0.0),
        );

        if metadata.layer_resolution.is_none() || metadata.shift_x.is_none() {
            if let Some(transform) = read_doubles(&header, &entries, bytes, TAG_MODEL_TRANSFORMATION)? {
                if transform.len() >= 16 {
                    let a = transform[0];
                    let e = transform[4];
                    let yaw = e.atan2(a);
                    shift.rotation = Quaternion::from_euler(datapod::Euler::new(0.0, 0.0, yaw));
                    let scale_x_deg = (a * a + e * e).sqrt();
                    resolution = deg_to_meters(&layer_datum, scale_x_deg);
                }
            } else if let Some(scales) = read_doubles(&header, &entries, bytes, TAG_MODEL_PIXEL_SCALE)? {
                if !scales.is_empty() {
                    resolution = deg_to_meters(&layer_datum, scales[0]);
                }
            }
        }

        if layers.is_empty() {
            collection_datum = Geo::new(
                metadata.collection_datum_lat.unwrap_or(layer_datum.latitude),
                metadata.collection_datum_lon.unwrap_or(layer_datum.longitude),
                metadata.collection_datum_alt.unwrap_or(layer_datum.altitude),
            );
            collection_shift = shift;
            collection_resolution = metadata.collection_resolution.unwrap_or(resolution);
        }

        let grid = decode_grid_data(
            &header,
            &pixel_bytes,
            width,
            height,
            resolution,
            shift,
            bits_per_sample,
            samples_per_pixel,
            photometric,
            sample_format,
        )?;
        let mut layer = Layer::new(grid);
        layer.ifd_offset = ifd_offset;
        layer.strip_offsets = strip_offsets;
        layer.strip_byte_counts = strip_byte_counts;
        layer.datum = layer_datum;
        layer.shift = shift;
        layer.resolution = resolution;
        layer.image_description = image_description;
        for (tag, values) in read_custom_tags(&header, &entries, bytes)? {
            layer.custom_tags.insert(tag, values);
        }
        for (key, value) in metadata.properties {
            layer.set_global_property(&key, &value);
        }

        let geo_ascii = read_geo_ascii_params(&header, &entries, bytes)?;
        let geo_keys = read_geo_key_directory(&header, &entries, bytes)?;
        if let Some(geo_ascii) = geo_ascii.as_ref() {
            if let Some(key) = geo_keys.get(&1026) {
                if let Some(text) = geo_key_ascii(key, geo_ascii) {
                    layer.geo_ascii_params = text;
                }
            }
            if let Some(key) = geo_keys.get(&4097) {
                if let Some(text) = geo_key_ascii(key, geo_ascii) {
                    layer.vertical_citation = text;
                }
            }
        }
        if let Some(key) = geo_keys.get(&4098) {
            if key.tiff_tag_location == 0 {
                layer.vertical_datum = Some(key.value_offset);
            }
        }
        if let Some(key) = geo_keys.get(&4099) {
            if key.tiff_tag_location == 0 {
                layer.vertical_units = Some(key.value_offset);
            }
        }
        if let Some(values) = read_geo_double_params(&header, &entries, bytes)? {
            layer.geo_double_params = values;
        }
        if let Some(value) = read_gdal_nodata(&header, &entries, bytes)? {
            layer.no_data_value = Some(value);
        }
        if photometric == 3 {
            if let Some(entry) = entries.get(&TAG_COLOR_MAP) {
                let total_count = usize::try_from(entry.count).map_err(|_| Error::Validation {
                    field: "ColorMap count",
                    expected: "value that fits in usize".to_owned(),
                    actual: entry.count.to_string(),
                })?;
                let entries_count = total_count / 3;
                let shorts = read_shorts_from_entry(&header, entry, bytes, total_count)?;
                let mut palette = Vec::with_capacity(entries_count);
                for i in 0..entries_count {
                    palette.push((
                        shorts[i],
                        shorts[entries_count + i],
                        shorts[2 * entries_count + i],
                    ));
                }
                layer.palette = Some(palette);
            }
        }

        layers.push(layer);

        ifd_offset = next_ifd;
    }

    Ok(RasterCollection {
        layers,
        datum: collection_datum,
        shift: collection_shift,
        resolution: collection_resolution,
    })
}

fn deg_to_meters(datum: &Geo, degrees: f64) -> f64 {
    use concord::{Enu, to_enu};
    let center = Geo::new(datum.latitude, datum.longitude, datum.altitude);
    let east = Geo::new(datum.latitude, datum.longitude + degrees, datum.altitude);
    let center_enu: Enu = to_enu(*datum, center);
    let east_enu: Enu = to_enu(*datum, east);
    east_enu.east() - center_enu.east()
}

fn read_ifd(
    header: &Header,
    bytes: &[u8],
    offset: usize,
) -> Result<(BTreeMap<u16, Entry>, u64)> {
    let mut entries = BTreeMap::new();
    let (entry_count, mut cursor): (u64, usize) = if header.bigtiff {
        (header.read_u64(bytes, offset)?, offset + 8)
    } else {
        (u64::from(header.read_u16(bytes, offset)?), offset + 2)
    };
    for _ in 0..entry_count {
        let tag = header.read_u16(bytes, cursor)?;
        let field_type = header.read_u16(bytes, cursor + 2)?;
        let (count, value_or_offset, inline_bytes, inline_capacity, step) = if header.bigtiff {
            let count = header.read_u64(bytes, cursor + 4)?;
            let raw = bytes
                .get(cursor + 12..cursor + 20)
                .ok_or_else(|| Error::Message("IFD entry truncated".to_owned()))?;
            let mut inline = [0u8; 8];
            inline.copy_from_slice(raw);
            let value = header.read_u64(bytes, cursor + 12)?;
            (count, value, inline, 8, 20)
        } else {
            let count = u64::from(header.read_u32(bytes, cursor + 4)?);
            let raw = bytes
                .get(cursor + 8..cursor + 12)
                .ok_or_else(|| Error::Message("IFD entry truncated".to_owned()))?;
            let mut inline = [0u8; 8];
            inline[..4].copy_from_slice(raw);
            let value = u64::from(header.read_u32(bytes, cursor + 8)?);
            (count, value, inline, 4, 12)
        };
        entries.insert(
            tag,
            Entry {
                field_type,
                count,
                value_or_offset,
                inline_bytes,
                inline_capacity,
            },
        );
        cursor += step;
    }
    let next_ifd = if header.bigtiff {
        header.read_u64(bytes, cursor)?
    } else {
        u64::from(header.read_u32(bytes, cursor)?)
    };
    Ok((entries, next_ifd))
}

fn read_scalar_u64(
    header: &Header,
    entries: &BTreeMap<u16, Entry>,
    tag: u16,
) -> Result<Option<u64>> {
    let Some(entry) = entries.get(&tag) else {
        return Ok(None);
    };
    if entry.count != 1 {
        return Err(Error::Unsupported {
            feature: format!("tag {tag} count {}", entry.count),
        });
    }
    match entry.field_type {
        TIFF_TYPE_BYTE => Ok(Some(u64::from(entry.inline_bytes[0]))),
        TIFF_TYPE_SHORT => Ok(Some(u64::from(entry.read_inline_u16(header, 0)))),
        TIFF_TYPE_LONG => Ok(Some(u64::from(entry.read_inline_u32(header, 0)))),
        TIFF_TYPE_LONG8 => Ok(Some(entry.read_inline_u64(header))),
        _ => Err(Error::Unsupported {
            feature: format!("tag {tag} type {}", entry.field_type),
        }),
    }
}

fn read_long_array(
    header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
    tag: u16,
) -> Result<Option<Vec<u64>>> {
    let Some(entry) = entries.get(&tag) else {
        return Ok(None);
    };
    let count = usize::try_from(entry.count).map_err(|_| Error::Validation {
        field: "LONG array count",
        expected: "value that fits in usize".to_owned(),
        actual: entry.count.to_string(),
    })?;
    let element_size: usize = match entry.field_type {
        TIFF_TYPE_SHORT => 2,
        TIFF_TYPE_LONG => 4,
        TIFF_TYPE_LONG8 => 8,
        _ => {
            return Err(Error::Unsupported {
                feature: format!("tag {tag} type {}", entry.field_type),
            });
        }
    };
    let total_bytes = count * element_size;
    let mut out = Vec::with_capacity(count);
    if total_bytes <= entry.inline_capacity {
        for i in 0..count {
            let v = match element_size {
                2 => u64::from(entry.read_inline_u16(header, i)),
                4 => u64::from(entry.read_inline_u32(header, i)),
                8 => entry.read_inline_u64(header),
                _ => unreachable!(),
            };
            out.push(v);
        }
    } else {
        let offset = usize::try_from(entry.value_or_offset).map_err(|_| Error::Validation {
            field: "array offset",
            expected: "value that fits in usize".to_owned(),
            actual: entry.value_or_offset.to_string(),
        })?;
        let end = offset
            .checked_add(total_bytes)
            .ok_or_else(|| Error::Message("array overflow".to_owned()))?;
        let slice = bytes
            .get(offset..end)
            .ok_or_else(|| Error::Message("array points outside file".to_owned()))?;
        for i in 0..count {
            let chunk = &slice[i * element_size..(i + 1) * element_size];
            let v = match element_size {
                2 => u64::from(if header.little_endian {
                    u16::from_le_bytes([chunk[0], chunk[1]])
                } else {
                    u16::from_be_bytes([chunk[0], chunk[1]])
                }),
                4 => u64::from(if header.little_endian {
                    u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                } else {
                    u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                }),
                8 => {
                    if header.little_endian {
                        u64::from_le_bytes([
                            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
                            chunk[7],
                        ])
                    } else {
                        u64::from_be_bytes([
                            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
                            chunk[7],
                        ])
                    }
                }
                _ => unreachable!(),
            };
            out.push(v);
        }
    }
    Ok(Some(out))
}

fn read_doubles(
    header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
    tag: u16,
) -> Result<Option<Vec<f64>>> {
    let Some(entry) = entries.get(&tag) else {
        return Ok(None);
    };
    if entry.field_type != TIFF_TYPE_DOUBLE {
        return Ok(None);
    }
    let count = usize::try_from(entry.count).map_err(|_| Error::Validation {
        field: "DOUBLE array count",
        expected: "value that fits in usize".to_owned(),
        actual: entry.count.to_string(),
    })?;
    let offset = usize::try_from(entry.value_or_offset).map_err(|_| Error::Validation {
        field: "DOUBLE array offset",
        expected: "value that fits in usize".to_owned(),
        actual: entry.value_or_offset.to_string(),
    })?;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(header.read_f64(bytes, offset + i * 8)?);
    }
    Ok(Some(out))
}

fn read_ascii_tag(
    _header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
    tag: u16,
) -> Result<Option<String>> {
    let Some(entry) = entries.get(&tag) else {
        return Ok(None);
    };
    if entry.field_type != TIFF_TYPE_ASCII {
        return Err(Error::Unsupported {
            feature: format!("tag {tag} type {}", entry.field_type),
        });
    }
    let count = usize::try_from(entry.count).map_err(|_| Error::Validation {
        field: "ASCII count",
        expected: "value that fits in usize".to_owned(),
        actual: entry.count.to_string(),
    })?;
    let raw = if count <= entry.inline_capacity {
        entry.inline_bytes[..count].to_vec()
    } else {
        let offset = usize::try_from(entry.value_or_offset).map_err(|_| Error::Validation {
            field: "ASCII offset",
            expected: "value that fits in usize".to_owned(),
            actual: entry.value_or_offset.to_string(),
        })?;
        bytes
            .get(offset..offset + count)
            .ok_or_else(|| Error::Message("ASCII tag points outside file".to_owned()))?
            .to_vec()
    };
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    Ok(Some(String::from_utf8_lossy(&raw[..end]).into_owned()))
}

fn read_custom_tags(
    header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
) -> Result<Vec<(u16, Vec<u32>)>> {
    let mut custom = Vec::new();
    for (&tag, entry) in entries {
        if is_standard_tag(tag) || tag < PRIVATE_TAG_MIN {
            continue;
        }
        let values = match entry.field_type {
            TIFF_TYPE_LONG => read_long_values_typed(header, entry, bytes)?,
            TIFF_TYPE_SHORT => read_short_values_typed(header, entry, bytes)?
                .into_iter()
                .map(u32::from)
                .collect(),
            _ => continue,
        };
        custom.push((tag, values));
    }
    Ok(custom)
}

fn read_long_values_typed(header: &Header, entry: &Entry, bytes: &[u8]) -> Result<Vec<u32>> {
    let count = usize::try_from(entry.count).map_err(|_| Error::Validation {
        field: "LONG array count",
        expected: "value that fits in usize".to_owned(),
        actual: entry.count.to_string(),
    })?;
    if count * 4 <= entry.inline_capacity {
        return Ok((0..count).map(|i| entry.read_inline_u32(header, i)).collect());
    }
    let offset = usize::try_from(entry.value_or_offset).map_err(|_| Error::Validation {
        field: "LONG array offset",
        expected: "value that fits in usize".to_owned(),
        actual: entry.value_or_offset.to_string(),
    })?;
    let end = offset
        .checked_add(count * 4)
        .ok_or_else(|| Error::Message("LONG array overflow".to_owned()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| Error::Message("LONG array points outside file".to_owned()))?;
    Ok(slice
        .chunks_exact(4)
        .map(|chunk| {
            if header.little_endian {
                u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
            } else {
                u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
            }
        })
        .collect())
}

fn read_short_values_typed(header: &Header, entry: &Entry, bytes: &[u8]) -> Result<Vec<u16>> {
    let count = usize::try_from(entry.count).map_err(|_| Error::Validation {
        field: "SHORT array count",
        expected: "value that fits in usize".to_owned(),
        actual: entry.count.to_string(),
    })?;
    if count * 2 <= entry.inline_capacity {
        return Ok((0..count).map(|i| entry.read_inline_u16(header, i)).collect());
    }
    let offset = usize::try_from(entry.value_or_offset).map_err(|_| Error::Validation {
        field: "SHORT array offset",
        expected: "value that fits in usize".to_owned(),
        actual: entry.value_or_offset.to_string(),
    })?;
    let end = offset
        .checked_add(count * 2)
        .ok_or_else(|| Error::Message("SHORT array overflow".to_owned()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| Error::Message("SHORT array points outside file".to_owned()))?;
    Ok(slice
        .chunks_exact(2)
        .map(|chunk| {
            if header.little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn decode_grid_data(
    header: &Header,
    bytes: &[u8],
    width: usize,
    height: usize,
    resolution: f64,
    pose: Pose,
    bits_per_sample: u16,
    samples_per_pixel: u16,
    photometric: u16,
    sample_format: SampleFormat,
) -> Result<GridData> {
    if samples_per_pixel == 4 && bits_per_sample == 8 && photometric == 2 {
        let pixels: Vec<Rgba8> = bytes
            .chunks_exact(4)
            .map(|c| Rgba8::new(c[0], c[1], c[2], c[3]))
            .collect();
        return Ok(GridData::Rgba8(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(pixels),
        }));
    }
    if samples_per_pixel == 3 && bits_per_sample == 8 && photometric == 2 {
        let pixels: Vec<Rgba8> = bytes
            .chunks_exact(3)
            .map(|c| Rgba8::new(c[0], c[1], c[2], 255))
            .collect();
        return Ok(GridData::Rgba8(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(pixels),
        }));
    }
    if samples_per_pixel != 1 {
        return Err(Error::Unsupported {
            feature: format!("SamplesPerPixel={samples_per_pixel}"),
        });
    }
    let cells = width
        .checked_mul(height)
        .ok_or_else(|| Error::Message("grid size overflow".to_owned()))?;
    let le = header.little_endian;
    let grid = match (bits_per_sample, sample_format) {
        (8, SampleFormat::UnsignedInt) => GridData::U8(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(bytes.to_vec()),
        }),
        (8, SampleFormat::SignedInt) => GridData::I8(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(bytes.iter().map(|b| *b as i8).collect::<Vec<_>>()),
        }),
        (16, SampleFormat::UnsignedInt) => GridData::U16(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(read_chunks(bytes, cells, |c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })?),
        }),
        (16, SampleFormat::SignedInt) => GridData::I16(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(read_chunks(bytes, cells, |c| {
                if le {
                    i16::from_le_bytes([c[0], c[1]])
                } else {
                    i16::from_be_bytes([c[0], c[1]])
                }
            })?),
        }),
        (32, SampleFormat::UnsignedInt) => GridData::U32(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(read_chunks(bytes, cells, |c| {
                if le {
                    u32::from_le_bytes([c[0], c[1], c[2], c[3]])
                } else {
                    u32::from_be_bytes([c[0], c[1], c[2], c[3]])
                }
            })?),
        }),
        (32, SampleFormat::SignedInt) => GridData::I32(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(read_chunks(bytes, cells, |c| {
                if le {
                    i32::from_le_bytes([c[0], c[1], c[2], c[3]])
                } else {
                    i32::from_be_bytes([c[0], c[1], c[2], c[3]])
                }
            })?),
        }),
        (32, SampleFormat::Float) => GridData::F32(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(read_chunks(bytes, cells, |c| {
                if le {
                    f32::from_le_bytes([c[0], c[1], c[2], c[3]])
                } else {
                    f32::from_be_bytes([c[0], c[1], c[2], c[3]])
                }
            })?),
        }),
        (64, SampleFormat::Float) => GridData::F64(Grid {
            rows: height,
            cols: width,
            resolution,
            centered: true,
            pose,
            data: Vector::from(read_chunks(bytes, cells, |c| {
                if le {
                    f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])
                } else {
                    f64::from_be_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])
                }
            })?),
        }),
        _ => {
            return Err(Error::Unsupported {
                feature: format!("BitsPerSample={bits_per_sample} SampleFormat={sample_format:?}"),
            });
        }
    };
    Ok(grid)
}

fn read_chunks<T>(bytes: &[u8], expected_count: usize, map: impl Fn(&[u8]) -> T) -> Result<Vec<T>> {
    if expected_count == 0 {
        return Ok(Vec::new());
    }
    let chunk_size = bytes.len() / expected_count;
    if chunk_size * expected_count != bytes.len() {
        return Err(Error::Validation {
            field: "typed pixel byte count",
            expected: format!("multiple of {expected_count}"),
            actual: bytes.len().to_string(),
        });
    }
    Ok(bytes.chunks_exact(chunk_size).map(map).collect())
}

fn is_standard_tag(tag: u16) -> bool {
    matches!(
        tag,
        256 | 257
            | 258
            | 259
            | 262
            | 270
            | 273
            | 277
            | 278
            | 279
            | 284
            | 320
            | 338
            | 339
            | 33_550
            | 33_922
            | 34_264
            | 34_735
            | 34_736
            | 34_737
            | 42_113
    )
}

#[derive(Default)]
struct ParsedMetadata {
    layer_resolution: Option<f64>,
    layer_datum_lat: Option<f64>,
    layer_datum_lon: Option<f64>,
    layer_datum_alt: Option<f64>,
    shift_x: Option<f64>,
    shift_y: Option<f64>,
    shift_z: Option<f64>,
    rot_w: Option<f64>,
    rot_x: Option<f64>,
    rot_y: Option<f64>,
    rot_z: Option<f64>,
    collection_resolution: Option<f64>,
    collection_datum_lat: Option<f64>,
    collection_datum_lon: Option<f64>,
    collection_datum_alt: Option<f64>,
    properties: Vec<(String, String)>,
}

impl ParsedMetadata {
    fn from_description(text: &str) -> Self {
        let mut out = Self::default();
        for part in text.split(';') {
            if let Some((key, value)) = part.strip_prefix("prop:").and_then(|s| s.split_once('=')) {
                out.properties.push((
                    unescape_description_value(key),
                    unescape_description_value(value),
                ));
                continue;
            }
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            let Ok(number) = value.parse::<f64>() else {
                continue;
            };
            match key {
                "layer_resolution" => out.layer_resolution = Some(number),
                "layer_datum_lat" => out.layer_datum_lat = Some(number),
                "layer_datum_lon" => out.layer_datum_lon = Some(number),
                "layer_datum_alt" => out.layer_datum_alt = Some(number),
                "shift_x" => out.shift_x = Some(number),
                "shift_y" => out.shift_y = Some(number),
                "shift_z" => out.shift_z = Some(number),
                "rot_w" => out.rot_w = Some(number),
                "rot_x" => out.rot_x = Some(number),
                "rot_y" => out.rot_y = Some(number),
                "rot_z" => out.rot_z = Some(number),
                "collection_resolution" => out.collection_resolution = Some(number),
                "collection_datum_lat" => out.collection_datum_lat = Some(number),
                "collection_datum_lon" => out.collection_datum_lon = Some(number),
                "collection_datum_alt" => out.collection_datum_alt = Some(number),
                _ => {}
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy)]
struct GeoKeyRecord {
    tiff_tag_location: u16,
    count: u16,
    value_offset: u16,
}

fn read_geo_key_directory(
    header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
) -> Result<BTreeMap<u16, GeoKeyRecord>> {
    let Some(entry) = entries.get(&TAG_GEO_KEY_DIRECTORY) else {
        return Ok(BTreeMap::new());
    };
    if entry.field_type != TIFF_TYPE_SHORT {
        return Ok(BTreeMap::new());
    }
    let count = usize::try_from(entry.count).map_err(|_| Error::Validation {
        field: "GeoKeyDirectory count",
        expected: "value that fits in usize".to_owned(),
        actual: entry.count.to_string(),
    })?;
    if count < 4 {
        return Ok(BTreeMap::new());
    }
    let shorts = read_shorts_from_entry(header, entry, bytes, count)?;
    let num_keys = usize::from(shorts[3]);
    let mut keys = BTreeMap::new();
    for i in 0..num_keys {
        let base = 4 + i * 4;
        if base + 4 > shorts.len() {
            break;
        }
        let key_id = shorts[base];
        keys.insert(
            key_id,
            GeoKeyRecord {
                tiff_tag_location: shorts[base + 1],
                count: shorts[base + 2],
                value_offset: shorts[base + 3],
            },
        );
    }
    Ok(keys)
}

fn read_shorts_from_entry(
    header: &Header,
    entry: &Entry,
    bytes: &[u8],
    count: usize,
) -> Result<Vec<u16>> {
    if count * 2 <= entry.inline_capacity {
        return Ok((0..count)
            .map(|i| entry.read_inline_u16(header, i))
            .collect());
    }
    let offset = usize::try_from(entry.value_or_offset).map_err(|_| Error::Validation {
        field: "SHORT array offset",
        expected: "value that fits in usize".to_owned(),
        actual: entry.value_or_offset.to_string(),
    })?;
    let end = offset
        .checked_add(count * 2)
        .ok_or_else(|| Error::Message("SHORT array overflow".to_owned()))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| Error::Message("SHORT array points outside file".to_owned()))?;
    Ok(slice
        .chunks_exact(2)
        .map(|chunk| {
            if header.little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect())
}

fn read_geo_ascii_params(
    _header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
) -> Result<Option<String>> {
    let Some(entry) = entries.get(&TAG_GEO_ASCII_PARAMS) else {
        return Ok(None);
    };
    if entry.field_type != TIFF_TYPE_ASCII {
        return Ok(None);
    }
    let count = usize::try_from(entry.count).map_err(|_| Error::Validation {
        field: "GeoAsciiParams count",
        expected: "value that fits in usize".to_owned(),
        actual: entry.count.to_string(),
    })?;
    let raw = if count <= entry.inline_capacity {
        entry.inline_bytes[..count].to_vec()
    } else {
        let offset = usize::try_from(entry.value_or_offset).map_err(|_| Error::Validation {
            field: "GeoAsciiParams offset",
            expected: "value that fits in usize".to_owned(),
            actual: entry.value_or_offset.to_string(),
        })?;
        bytes
            .get(offset..offset + count)
            .ok_or_else(|| Error::Message("GeoAsciiParams points outside file".to_owned()))?
            .to_vec()
    };
    Ok(Some(String::from_utf8_lossy(&raw).into_owned()))
}

fn geo_key_ascii(key: &GeoKeyRecord, geo_ascii: &str) -> Option<String> {
    if key.tiff_tag_location != TAG_GEO_ASCII_PARAMS {
        return None;
    }
    let offset = usize::from(key.value_offset);
    let count = usize::from(key.count);
    if count == 0 || offset >= geo_ascii.len() {
        return Some(String::new());
    }
    let end = (offset + count).min(geo_ascii.len());
    let slice = &geo_ascii.as_bytes()[offset..end];
    let trimmed = if slice.last() == Some(&0) {
        &slice[..slice.len() - 1]
    } else {
        slice
    };
    Some(String::from_utf8_lossy(trimmed).into_owned())
}

fn read_geo_double_params(
    header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
) -> Result<Option<Vec<f64>>> {
    read_doubles(header, entries, bytes, TAG_GEO_DOUBLE_PARAMS)
}

fn read_gdal_nodata(
    header: &Header,
    entries: &BTreeMap<u16, Entry>,
    bytes: &[u8],
) -> Result<Option<f64>> {
    let Some(raw) = read_ascii_tag(header, entries, bytes, TAG_GDAL_NODATA)? else {
        return Ok(None);
    };
    Ok(raw.trim().parse::<f64>().ok())
}

fn unescape_description_value(value: &str) -> String {
    value
        .replace("%3B", ";")
        .replace("%3D", "=")
        .replace("%25", "%")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use datapod::{Geo, Grid, Point, Pose, Quaternion, Vector};

    use super::read_raster_collection;
    use crate::{GridData, Layer, RasterCollection, WriteOptions, write_raster_collection};
    use crate::color::Rgba8;

    #[test]
    fn round_trip_single_layer_u8_tiff() {
        let grid = Grid {
            rows: 2,
            cols: 3,
            resolution: 2.5,
            centered: true,
            pose: Pose {
                point: Point::new(1.0, 2.0, 3.0),
                rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            },
            data: Vector::from(vec![10u8, 20, 30, 40, 50, 60]),
        };
        let mut layer = Layer::new(GridData::from(grid));
        layer.datum = Geo::new(47.5, 8.5, 200.0);
        layer.shift = Pose {
            point: Point::new(1.0, 2.0, 3.0),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
        };
        layer.resolution = 2.5;
        layer.set_global_property("name", "terrain");
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::new(47.5, 8.5, 200.0),
            shift: Pose::default(),
            resolution: 2.5,
        };

        let path = std::env::temp_dir().join("rastera_roundtrip_test.tif");
        write_raster_collection(&collection, &path, &WriteOptions::default()).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        assert_eq!(parsed.layers.len(), 1);
        assert_eq!(parsed.layers[0].width(), 3);
        assert_eq!(parsed.layers[0].height(), 2);
        match &parsed.layers[0].grid {
            GridData::U8(grid) => assert_eq!(grid.data.as_slice(), &[10, 20, 30, 40, 50, 60]),
            _ => panic!("expected u8 grid"),
        }
        assert_eq!(parsed.resolution, 2.5);
        assert_eq!(parsed.datum.latitude, 47.5);
        assert_eq!(
            parsed.layers[0].get_global_properties().get("name"),
            Some(&"terrain".to_owned())
        );
    }

    #[test]
    fn round_trip_multi_layer_u8_tiff() {
        let layer1 = Layer::new(GridData::from(Grid {
            rows: 1,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![1u8, 2]),
        }));
        let mut layer2 = Layer::new(GridData::from(Grid {
            rows: 1,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![3u8, 4]),
        }));
        layer2.set_global_property("name", "second");
        let collection = RasterCollection {
            layers: vec![layer1, layer2],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let path = std::env::temp_dir().join("rastera_multi_roundtrip_test.tif");
        write_raster_collection(&collection, &path, &WriteOptions::default()).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        assert_eq!(parsed.layers.len(), 2);
        assert_eq!(
            parsed.layers[1].get_global_properties().get("name"),
            Some(&"second".to_owned())
        );
    }

    #[test]
    fn round_trip_i16_and_custom_tags() {
        let mut layer = Layer::new(GridData::from(Grid {
            rows: 1,
            cols: 3,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![-2i16, 0, 17]),
        }));
        layer.custom_tags.insert(50001, vec![42, 43]);
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let path = std::env::temp_dir().join("rastera_i16_roundtrip_test.tif");
        write_raster_collection(&collection, &path, &WriteOptions::default()).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        match &parsed.layers[0].grid {
            GridData::I16(grid) => assert_eq!(grid.data.as_slice(), &[-2, 0, 17]),
            _ => panic!("expected i16 grid"),
        }
        assert_eq!(
            parsed.layers[0].custom_tags.get(&50001),
            Some(&vec![42, 43])
        );
    }

    #[test]
    fn round_trip_gdal_nodata_and_geo_ascii() {
        let mut layer = Layer::new(GridData::from(Grid {
            rows: 1,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![1u8, 2]),
        }));
        layer.datum = Geo::new(47.5, 8.5, 200.0);
        layer.resolution = 1.0;
        layer.no_data_value = Some(-9999.0);
        layer.geo_ascii_params = "WGS 84 (from rastera)".to_owned();
        layer.vertical_citation = "Ellipsoid height".to_owned();
        layer.vertical_datum = Some(5030);
        layer.vertical_units = Some(9001);
        layer.geo_double_params = vec![1.5, -2.25];

        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::new(47.5, 8.5, 200.0),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let path = std::env::temp_dir().join("rastera_geotiff_tags.tif");
        write_raster_collection(&collection, &path, &WriteOptions::default()).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        let out = &parsed.layers[0];
        assert_eq!(out.no_data_value, Some(-9999.0));
        assert_eq!(out.geo_ascii_params, "WGS 84 (from rastera)");
        assert_eq!(out.vertical_citation, "Ellipsoid height");
        assert_eq!(out.vertical_datum, Some(5030));
        assert_eq!(out.vertical_units, Some(9001));
        assert_eq!(out.geo_double_params, vec![1.5, -2.25]);
    }

    #[test]
    fn emits_real_geotiff_model_tags() {
        let mut layer = Layer::new(GridData::from(Grid {
            rows: 4,
            cols: 4,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![0u8; 16]),
        }));
        layer.datum = Geo::new(47.5, 8.5, 200.0);
        layer.resolution = 1.0;

        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::new(47.5, 8.5, 200.0),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let bytes = crate::to_tiff_bytes(&collection, &crate::WriteOptions::default()).unwrap();
        let first_ifd = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let entry_count = u16::from_le_bytes([bytes[first_ifd], bytes[first_ifd + 1]]) as usize;
        let mut tags = std::collections::BTreeSet::new();
        for i in 0..entry_count {
            let off = first_ifd + 2 + i * 12;
            tags.insert(u16::from_le_bytes([bytes[off], bytes[off + 1]]));
        }
        assert!(tags.contains(&33_550));
        assert!(tags.contains(&33_922));
        assert!(tags.contains(&34_735));
    }

    #[test]
    fn round_trip_f32() {
        let layer = Layer::new(GridData::from(Grid {
            rows: 1,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![1.5f32, -2.25]),
        }));
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let path = std::env::temp_dir().join("rastera_f32_roundtrip_test.tif");
        write_raster_collection(&collection, &path, &WriteOptions::default()).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        match &parsed.layers[0].grid {
            GridData::F32(grid) => {
                assert!((grid.data[0] - 1.5).abs() < 1e-6);
                assert!((grid.data[1] + 2.25).abs() < 1e-6);
            }
            _ => panic!("expected f32 grid"),
        }
    }

    #[test]
    fn round_trip_rgba_tiff() {
        let pixels: Vec<Rgba8> = (0..12)
            .map(|i| Rgba8::new(i as u8, (i * 2) as u8, (i * 3) as u8, 255))
            .collect();
        let grid = Grid {
            rows: 3,
            cols: 4,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(pixels.clone()),
        };
        let layer = Layer::new(GridData::from(grid));
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let path = std::env::temp_dir().join("rastera_rgba_roundtrip.tif");
        write_raster_collection(&collection, &path, &WriteOptions::default()).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        match &parsed.layers[0].grid {
            GridData::Rgba8(g) => {
                assert_eq!(g.rows, 3);
                assert_eq!(g.cols, 4);
                for (a, b) in g.data.as_slice().iter().zip(pixels.iter()) {
                    assert_eq!(a.r, b.r);
                    assert_eq!(a.g, b.g);
                    assert_eq!(a.b, b.b);
                    assert_eq!(a.a, b.a);
                }
            }
            _ => panic!("expected rgba grid"),
        }
    }

    #[test]
    fn round_trip_multi_strip_tiff() {
        let rows = 40;
        let cols = 200;
        let data: Vec<u8> = (0..rows * cols).map(|i| (i % 251) as u8).collect();
        let grid = Grid {
            rows,
            cols,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(data.clone()),
        };
        let layer = Layer::new(GridData::from(grid));
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let options = WriteOptions {
            rows_per_strip: 5,
            ..WriteOptions::default()
        };
        let path = std::env::temp_dir().join("rastera_multi_strip.tif");
        write_raster_collection(&collection, &path, &options).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        match &parsed.layers[0].grid {
            GridData::U8(g) => assert_eq!(g.data.as_slice(), data.as_slice()),
            _ => panic!("expected u8 grid"),
        }
        assert!(parsed.layers[0].strip_offsets.len() > 1);
    }

    #[test]
    fn round_trip_rotated_grid() {
        let grid = Grid {
            rows: 8,
            cols: 8,
            resolution: 1.0,
            centered: true,
            pose: Pose {
                point: Point::new(0.0, 0.0, 0.0),
                rotation: Quaternion::from_euler(datapod::Euler::new(0.0, 0.0, std::f64::consts::FRAC_PI_4)),
            },
            data: Vector::from(vec![7u8; 64]),
        };
        let mut layer = Layer::new(GridData::from(grid));
        layer.datum = Geo::new(52.0, 5.0, 10.0);
        layer.resolution = 1.0;
        layer.shift = Pose {
            point: Point::new(0.0, 0.0, 0.0),
            rotation: Quaternion::from_euler(datapod::Euler::new(0.0, 0.0, std::f64::consts::FRAC_PI_4)),
        };
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::new(52.0, 5.0, 10.0),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let bytes = crate::to_tiff_bytes(&collection, &WriteOptions::default()).unwrap();
        let first_ifd = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        let entry_count = u16::from_le_bytes([bytes[first_ifd], bytes[first_ifd + 1]]) as usize;
        let mut tags = std::collections::BTreeSet::new();
        for i in 0..entry_count {
            let off = first_ifd + 2 + i * 12;
            tags.insert(u16::from_le_bytes([bytes[off], bytes[off + 1]]));
        }
        assert!(tags.contains(&34_264));
        assert!(!tags.contains(&33_550));
    }

    #[test]
    fn parses_big_endian_minimal_tiff() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MM");
        bytes.extend_from_slice(&42u16.to_be_bytes());
        bytes.extend_from_slice(&10u32.to_be_bytes());
        bytes.extend_from_slice(&[0xAA, 0xBB]);
        let entries_offset = bytes.len();
        let entry_count = 12u16;
        bytes.extend_from_slice(&entry_count.to_be_bytes());
        let add_entry = |bytes: &mut Vec<u8>, tag: u16, ty: u16, count: u32, value: u32| {
            bytes.extend_from_slice(&tag.to_be_bytes());
            bytes.extend_from_slice(&ty.to_be_bytes());
            bytes.extend_from_slice(&count.to_be_bytes());
            bytes.extend_from_slice(&value.to_be_bytes());
        };
        add_entry(&mut bytes, 256, 4, 1, 2);
        add_entry(&mut bytes, 257, 4, 1, 1);
        add_entry(&mut bytes, 258, 3, 1, 8 << 16);
        add_entry(&mut bytes, 259, 3, 1, 1 << 16);
        add_entry(&mut bytes, 262, 3, 1, 1 << 16);
        add_entry(&mut bytes, 273, 4, 1, 8);
        add_entry(&mut bytes, 277, 3, 1, 1 << 16);
        add_entry(&mut bytes, 278, 4, 1, 1);
        add_entry(&mut bytes, 279, 4, 1, 2);
        add_entry(&mut bytes, 284, 3, 1, 1 << 16);
        add_entry(&mut bytes, 339, 3, 1, 1 << 16);
        add_entry(&mut bytes, super::TAG_GEO_KEY_DIRECTORY, 3, 20, 0);
        bytes.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(entries_offset, 10);

        let collection = super::parse_raster_collection(&bytes).unwrap();
        match &collection.layers[0].grid {
            GridData::U8(g) => {
                assert_eq!(g.rows, 1);
                assert_eq!(g.cols, 2);
                assert_eq!(g.data.as_slice(), &[0xAA, 0xBB]);
            }
            _ => panic!("expected u8 grid"),
        }
    }

    #[test]
    fn parses_rgb_three_channel_as_rgba() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&20u32.to_le_bytes());
        bytes.extend_from_slice(&[
            10, 20, 30,
            40, 50, 60,
            70, 80, 90,
            100, 110, 120,
        ]);
        assert_eq!(bytes.len(), 20);
        let entry_count: u16 = 11;
        bytes.extend_from_slice(&entry_count.to_le_bytes());
        let add = |out: &mut Vec<u8>, tag: u16, ty: u16, count: u32, value: u32| {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&ty.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        };
        add(&mut bytes, 256, 4, 1, 2);
        add(&mut bytes, 257, 4, 1, 2);
        add(&mut bytes, 258, 3, 1, 8);
        add(&mut bytes, 259, 3, 1, 1);
        add(&mut bytes, 262, 3, 1, 2);
        add(&mut bytes, 273, 4, 1, 8);
        add(&mut bytes, 277, 3, 1, 3);
        add(&mut bytes, 278, 4, 1, 2);
        add(&mut bytes, 279, 4, 1, 12);
        add(&mut bytes, 284, 3, 1, 1);
        add(&mut bytes, 339, 3, 1, 1);
        bytes.extend_from_slice(&0u32.to_le_bytes());

        let collection = super::parse_raster_collection(&bytes).unwrap();
        match &collection.layers[0].grid {
            GridData::Rgba8(g) => {
                assert_eq!(g.rows, 2);
                assert_eq!(g.cols, 2);
                assert_eq!(g.data[0], Rgba8::new(10, 20, 30, 255));
                assert_eq!(g.data[3], Rgba8::new(100, 110, 120, 255));
            }
            _ => panic!("expected rgba grid"),
        }
    }

    #[test]
    fn round_trip_planar_rgba() {
        let pixels: Vec<Rgba8> = (0..6)
            .map(|i| Rgba8::new(10 + i as u8, 100 + i as u8, 200 - i as u8, 250 + i as u8))
            .collect();
        let grid = Grid {
            rows: 2,
            cols: 3,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(pixels.clone()),
        };
        let layer = Layer::new(GridData::from(grid));
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };
        let options = WriteOptions {
            planar_config: 2,
            ..WriteOptions::default()
        };
        let path = std::env::temp_dir().join("rastera_planar_rgba.tif");
        write_raster_collection(&collection, &path, &options).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();
        match &parsed.layers[0].grid {
            GridData::Rgba8(g) => {
                for (got, exp) in g.data.as_slice().iter().zip(pixels.iter()) {
                    assert_eq!(got.r, exp.r);
                    assert_eq!(got.g, exp.g);
                    assert_eq!(got.b, exp.b);
                    assert_eq!(got.a, exp.a);
                }
            }
            _ => panic!("expected rgba grid"),
        }
    }

    #[test]
    fn round_trip_palette_u8() {
        let grid = Grid {
            rows: 2,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![0u8, 1, 2, 3]),
        };
        let mut layer = Layer::new(GridData::from(grid));
        let mut palette = vec![(0u16, 0u16, 0u16); 256];
        palette[0] = (0, 0, 0);
        palette[1] = (65535, 0, 0);
        palette[2] = (0, 65535, 0);
        palette[3] = (0, 0, 65535);
        layer.palette = Some(palette);
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };
        let path = std::env::temp_dir().join("rastera_palette.tif");
        write_raster_collection(&collection, &path, &WriteOptions::default()).unwrap();
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        let out = &parsed.layers[0];
        match &out.grid {
            GridData::U8(g) => assert_eq!(g.data.as_slice(), &[0u8, 1, 2, 3]),
            _ => panic!("expected u8 grid"),
        }
        let out_palette = out.palette.as_ref().expect("palette round-trip");
        assert_eq!(out_palette.len(), 256);
        assert_eq!(out_palette[0], (0, 0, 0));
        assert_eq!(out_palette[1], (65535, 0, 0));
        assert_eq!(out_palette[2], (0, 65535, 0));
        assert_eq!(out_palette[3], (0, 0, 65535));
    }

    #[test]
    fn round_trip_big_endian_writer() {
        let layer = Layer::new(GridData::from(Grid {
            rows: 2,
            cols: 3,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![-4000i16, -1, 0, 1, 2, 30000]),
        }));
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };
        let options = WriteOptions {
            big_endian: true,
            ..WriteOptions::default()
        };
        let path = std::env::temp_dir().join("rastera_be_roundtrip.tif");
        write_raster_collection(&collection, &path, &options).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[0..2], b"MM");
        assert_eq!(u16::from_be_bytes([bytes[2], bytes[3]]), 42);
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();
        match &parsed.layers[0].grid {
            GridData::I16(g) => assert_eq!(g.data.as_slice(), &[-4000, -1, 0, 1, 2, 30000]),
            _ => panic!("expected i16 grid"),
        }
    }

    #[test]
    fn round_trip_bigtiff() {
        let grid = Grid {
            rows: 2,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![1u8, 2, 3, 4]),
        };
        let layer = Layer::new(GridData::from(grid));
        let collection = RasterCollection {
            layers: vec![layer],
            datum: Geo::default(),
            shift: Pose::default(),
            resolution: 1.0,
        };

        let options = WriteOptions {
            force_bigtiff: true,
            ..WriteOptions::default()
        };
        let path = std::env::temp_dir().join("rastera_bigtiff.tif");
        write_raster_collection(&collection, &path, &options).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(u16::from_le_bytes([bytes[2], bytes[3]]), 43);
        let parsed = read_raster_collection(&path).unwrap();
        fs::remove_file(&path).ok();

        match &parsed.layers[0].grid {
            GridData::U8(g) => assert_eq!(g.data.as_slice(), &[1, 2, 3, 4]),
            _ => panic!("expected u8 grid"),
        }
    }
}
