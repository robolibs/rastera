use std::collections::{BTreeMap, HashMap};

use datapod::{Encoding, Geo, Grid, Point, Pose};

use crate::tags::{GLOBAL_PROPERTIES_BASE_TAG, validate_custom_tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SampleFormat {
    UnsignedInt = 1,
    SignedInt = 2,
    Float = 3,
    Undefined = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum PhotometricInterpretation {
    WhiteIsZero = 0,
    BlackIsZero = 1,
    Rgb = 2,
    Palette = 3,
    Mask = 4,
    Cmyk = 5,
    YCbCr = 6,
    CieLab = 8,
}

/// Tagged-union of grid storage by cell type. Each variant wraps a single
/// `datapod::Grid` — the Grid owns its bytes via the inside `data: Vec<u8>`
/// field. The variant tag is redundant with `grid.encoding` but provides
/// Rust-side type-matching ergonomics for the parser/writer pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum GridData {
    U8(Grid),
    I8(Grid),
    U16(Grid),
    I16(Grid),
    U32(Grid),
    I32(Grid),
    F32(Grid),
    F64(Grid),
    Rgba8(Grid),
}

impl From<Grid> for GridData {
    fn from(g: Grid) -> Self {
        match g.encoding {
            Encoding::U8 | Encoding::Mono8 => Self::U8(g),
            Encoding::I8 => Self::I8(g),
            Encoding::U16 | Encoding::Mono16 => Self::U16(g),
            Encoding::I16 => Self::I16(g),
            Encoding::U32 => Self::U32(g),
            Encoding::I32 => Self::I32(g),
            Encoding::F32 => Self::F32(g),
            Encoding::F64 => Self::F64(g),
            Encoding::Rgba8 | Encoding::Rgb8 => Self::Rgba8(g),
            // U64 / I64 don't have a dedicated GridData variant; treat as U8.
            Encoding::U64 | Encoding::I64 => Self::U8(g),
        }
    }
}

impl GridData {
    pub fn grid(&self) -> &Grid {
        match self {
            Self::U8(g)
            | Self::I8(g)
            | Self::U16(g)
            | Self::I16(g)
            | Self::U32(g)
            | Self::I32(g)
            | Self::F32(g)
            | Self::F64(g)
            | Self::Rgba8(g) => g,
        }
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        match self {
            Self::U8(g)
            | Self::I8(g)
            | Self::U16(g)
            | Self::I16(g)
            | Self::U32(g)
            | Self::I32(g)
            | Self::F32(g)
            | Self::F64(g)
            | Self::Rgba8(g) => g,
        }
    }

    pub fn bits_per_sample(&self) -> u16 {
        match self {
            Self::U8(_) | Self::I8(_) | Self::Rgba8(_) => 8,
            Self::U16(_) | Self::I16(_) => 16,
            Self::U32(_) | Self::I32(_) | Self::F32(_) => 32,
            Self::F64(_) => 64,
        }
    }

    pub fn sample_format(&self) -> SampleFormat {
        match self {
            Self::U8(_) | Self::U16(_) | Self::U32(_) | Self::Rgba8(_) => SampleFormat::UnsignedInt,
            Self::I8(_) | Self::I16(_) | Self::I32(_) => SampleFormat::SignedInt,
            Self::F32(_) | Self::F64(_) => SampleFormat::Float,
        }
    }

    pub fn samples_per_pixel(&self) -> u16 {
        match self {
            Self::Rgba8(_) => 4,
            _ => 1,
        }
    }

    pub fn photometric_interpretation(&self) -> PhotometricInterpretation {
        match self {
            Self::Rgba8(_) => PhotometricInterpretation::Rgb,
            _ => PhotometricInterpretation::BlackIsZero,
        }
    }

    pub fn is_color(&self) -> bool {
        matches!(self, Self::Rgba8(_))
    }

    pub fn dimensions(&self) -> (usize, usize) {
        let g = self.grid();
        (g.rows as usize, g.cols as usize)
    }

    pub fn resolution(&self) -> f64 {
        self.grid().resolution
    }

    pub fn pose(&self) -> Pose {
        self.grid().pose
    }

    pub fn get_point(&self, row: usize, col: usize) -> Point {
        self.grid().get_point(row, col)
    }
}

pub fn string_to_ascii_tag(value: &str) -> Vec<u32> {
    let mut padded = value.as_bytes().to_vec();
    padded.push(0);
    while padded.len() % 4 != 0 {
        padded.push(0);
    }

    padded
        .chunks_exact(4)
        .map(|chunk| {
            u32::from(chunk[0])
                | (u32::from(chunk[1]) << 8)
                | (u32::from(chunk[2]) << 16)
                | (u32::from(chunk[3]) << 24)
        })
        .collect()
}

pub fn ascii_tag_to_string(values: &[u32]) -> String {
    let mut result = String::with_capacity(values.len() * 4);

    for value in values {
        for shift in [0, 8, 16, 24] {
            let byte = ((value >> shift) & 0xff) as u8;
            if byte == 0 {
                return result;
            }
            result.push(char::from(byte));
        }
    }

    result
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub ifd_offset: u64,
    pub strip_offsets: Vec<u64>,
    pub strip_byte_counts: Vec<u64>,
    pub datum: Geo,
    pub shift: Pose,
    pub resolution: f64,
    pub image_description: String,
    pub custom_tags: BTreeMap<u16, Vec<u32>>,
    pub no_data_value: Option<f64>,
    pub geo_ascii_params: String,
    pub geo_double_params: Vec<f64>,
    pub vertical_datum: Option<u16>,
    pub vertical_units: Option<u16>,
    pub vertical_citation: String,
    pub palette: Option<Vec<(u16, u16, u16)>>,
    pub grid: GridData,
}

impl Layer {
    pub fn new(grid: GridData) -> Self {
        let resolution = grid.resolution();
        let shift = grid.pose();
        Self {
            ifd_offset: 0,
            strip_offsets: Vec::new(),
            strip_byte_counts: Vec::new(),
            datum: Geo::default(),
            shift,
            resolution,
            image_description: String::new(),
            custom_tags: BTreeMap::new(),
            no_data_value: None,
            geo_ascii_params: String::new(),
            geo_double_params: Vec::new(),
            vertical_datum: None,
            vertical_units: None,
            vertical_citation: String::new(),
            palette: None,
            grid,
        }
    }

    pub fn width(&self) -> usize {
        self.grid.dimensions().1
    }

    pub fn height(&self) -> usize {
        self.grid.dimensions().0
    }

    pub fn samples_per_pixel(&self) -> u16 {
        self.grid.samples_per_pixel()
    }

    pub fn bits_per_sample(&self) -> u16 {
        self.grid.bits_per_sample()
    }

    pub fn sample_format(&self) -> SampleFormat {
        self.grid.sample_format()
    }

    pub fn photometric_interpretation(&self) -> PhotometricInterpretation {
        self.grid.photometric_interpretation()
    }

    pub fn set_custom_tag(&mut self, tag: u16, values: Vec<u32>) -> crate::Result<()> {
        validate_custom_tag(tag)?;
        self.custom_tags.insert(tag, values);
        Ok(())
    }

    pub fn get_custom_tag(&self, tag: u16) -> Option<&[u32]> {
        self.custom_tags.get(&tag).map(Vec::as_slice)
    }

    pub fn has_custom_tag(&self, tag: u16) -> bool {
        self.custom_tags.contains_key(&tag)
    }

    pub fn set_global_property(&mut self, key: &str, value: &str) {
        let tag = global_property_tag(key);
        self.custom_tags
            .insert(tag, string_to_ascii_tag(&format!("{key}={value}")));
    }

    pub fn get_global_properties(&self) -> HashMap<String, String> {
        self.custom_tags
            .iter()
            .filter(|(tag, _)| {
                **tag >= GLOBAL_PROPERTIES_BASE_TAG && **tag < GLOBAL_PROPERTIES_BASE_TAG + 1000
            })
            .filter_map(|(_, values)| {
                let decoded = ascii_tag_to_string(values);
                decoded
                    .split_once('=')
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
            })
            .collect()
    }
}

pub fn global_property_tag(key: &str) -> u16 {
    let hash: u32 = key
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    GLOBAL_PROPERTIES_BASE_TAG + (hash % 1000) as u16
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RasterCollection {
    pub layers: Vec<Layer>,
    pub datum: Geo,
    pub shift: Pose,
    pub resolution: f64,
}

impl RasterCollection {
    pub fn set_global_properties_on_all_layers(&mut self, props: &HashMap<String, String>) {
        for layer in &mut self.layers {
            for (k, v) in props {
                layer.set_global_property(k, v);
            }
        }
    }
}

/// Helper to build a Grid with given metadata + raw byte buffer.
pub fn make_grid(
    rows: u32,
    cols: u32,
    encoding: Encoding,
    resolution: f64,
    centered: bool,
    pose: Pose,
    data: Vec<u8>,
) -> Grid {
    Grid {
        rows,
        cols,
        encoding,
        centered: if centered { 1 } else { 0 },
        resolution,
        pose,
        data,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use datapod::{Encoding, Pose};

    use super::{
        GridData, Layer, PhotometricInterpretation, RasterCollection, SampleFormat,
        ascii_tag_to_string, global_property_tag, make_grid, string_to_ascii_tag,
    };

    fn grid_u8(rows: u32, cols: u32) -> GridData {
        let g = make_grid(
            rows,
            cols,
            Encoding::U8,
            2.0,
            true,
            Pose::default(),
            vec![0u8; (rows * cols) as usize],
        );
        GridData::U8(g)
    }

    #[test]
    fn ascii_tags_round_trip() {
        let encoded = string_to_ascii_tag("layer=name");
        assert_eq!(ascii_tag_to_string(&encoded), "layer=name");
    }

    #[test]
    fn grid_data_reports_expected_metadata() {
        let data = grid_u8(3, 4);
        assert_eq!(data.dimensions(), (3, 4));
        assert_eq!(data.bits_per_sample(), 8);
        assert_eq!(data.sample_format(), SampleFormat::UnsignedInt);
        assert_eq!(
            data.photometric_interpretation(),
            PhotometricInterpretation::BlackIsZero
        );
    }

    #[test]
    fn layer_derives_shape_from_grid() {
        let layer = Layer::new(grid_u8(5, 7));
        assert_eq!(layer.height(), 5);
        assert_eq!(layer.width(), 7);
        assert_eq!(layer.samples_per_pixel(), 1);
    }

    #[test]
    fn layer_global_properties_round_trip() {
        let mut layer = Layer::new(grid_u8(2, 2));
        layer.set_global_property("name", "terrain");
        assert_eq!(
            layer.get_global_properties().get("name"),
            Some(&"terrain".to_owned())
        );
    }

    #[test]
    fn collection_propagates_global_properties() {
        let mut collection = RasterCollection {
            layers: vec![Layer::new(grid_u8(1, 1)), Layer::new(grid_u8(1, 1))],
            ..RasterCollection::default()
        };
        let mut props = HashMap::new();
        props.insert("mission".to_owned(), "alpha".to_owned());

        collection.set_global_properties_on_all_layers(&props);

        for layer in &collection.layers {
            assert_eq!(
                layer.get_global_properties().get("mission"),
                Some(&"alpha".to_owned())
            );
        }
    }

    #[test]
    fn global_property_tag_is_stable() {
        assert_eq!(
            global_property_tag("mission"),
            global_property_tag("mission")
        );
        assert_ne!(global_property_tag("mission"), global_property_tag("name"));
    }
}
