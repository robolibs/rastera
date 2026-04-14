use std::collections::{BTreeMap, HashMap};

use datapod::{Geo, Grid, Layer as SpatialLayer, Point, Pose, Vector};

use crate::color::Rgba8;
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

#[derive(Debug, Clone, PartialEq)]
pub enum GridData {
    U8(Grid<u8>),
    I8(Grid<i8>),
    U16(Grid<u16>),
    I16(Grid<i16>),
    U32(Grid<u32>),
    I32(Grid<i32>),
    F32(Grid<f32>),
    F64(Grid<f64>),
    Rgba8(Grid<Rgba8>),
}

impl GridData {
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
        match self {
            Self::U8(grid) => (grid.rows, grid.cols),
            Self::I8(grid) => (grid.rows, grid.cols),
            Self::U16(grid) => (grid.rows, grid.cols),
            Self::I16(grid) => (grid.rows, grid.cols),
            Self::U32(grid) => (grid.rows, grid.cols),
            Self::I32(grid) => (grid.rows, grid.cols),
            Self::F32(grid) => (grid.rows, grid.cols),
            Self::F64(grid) => (grid.rows, grid.cols),
            Self::Rgba8(grid) => (grid.rows, grid.cols),
        }
    }

    pub fn resolution(&self) -> f64 {
        match self {
            Self::U8(grid) => grid.resolution,
            Self::I8(grid) => grid.resolution,
            Self::U16(grid) => grid.resolution,
            Self::I16(grid) => grid.resolution,
            Self::U32(grid) => grid.resolution,
            Self::I32(grid) => grid.resolution,
            Self::F32(grid) => grid.resolution,
            Self::F64(grid) => grid.resolution,
            Self::Rgba8(grid) => grid.resolution,
        }
    }

    pub fn pose(&self) -> Pose {
        match self {
            Self::U8(grid) => grid.pose,
            Self::I8(grid) => grid.pose,
            Self::U16(grid) => grid.pose,
            Self::I16(grid) => grid.pose,
            Self::U32(grid) => grid.pose,
            Self::I32(grid) => grid.pose,
            Self::F32(grid) => grid.pose,
            Self::F64(grid) => grid.pose,
            Self::Rgba8(grid) => grid.pose,
        }
    }

    pub fn get_point(&self, row: usize, col: usize) -> Point {
        match self {
            Self::U8(grid) => grid.get_point(row, col),
            Self::I8(grid) => grid.get_point(row, col),
            Self::U16(grid) => grid.get_point(row, col),
            Self::I16(grid) => grid.get_point(row, col),
            Self::U32(grid) => grid.get_point(row, col),
            Self::I32(grid) => grid.get_point(row, col),
            Self::F32(grid) => grid.get_point(row, col),
            Self::F64(grid) => grid.get_point(row, col),
            Self::Rgba8(grid) => grid.get_point(row, col),
        }
    }
}

impl From<Grid<u8>> for GridData {
    fn from(value: Grid<u8>) -> Self {
        Self::U8(value)
    }
}

impl From<Grid<i8>> for GridData {
    fn from(value: Grid<i8>) -> Self {
        Self::I8(value)
    }
}

impl From<Grid<u16>> for GridData {
    fn from(value: Grid<u16>) -> Self {
        Self::U16(value)
    }
}

impl From<Grid<i16>> for GridData {
    fn from(value: Grid<i16>) -> Self {
        Self::I16(value)
    }
}

impl From<Grid<u32>> for GridData {
    fn from(value: Grid<u32>) -> Self {
        Self::U32(value)
    }
}

impl From<Grid<i32>> for GridData {
    fn from(value: Grid<i32>) -> Self {
        Self::I32(value)
    }
}

impl From<Grid<f32>> for GridData {
    fn from(value: Grid<f32>) -> Self {
        Self::F32(value)
    }
}

impl From<Grid<f64>> for GridData {
    fn from(value: Grid<f64>) -> Self {
        Self::F64(value)
    }
}

impl From<Grid<Rgba8>> for GridData {
    fn from(value: Grid<Rgba8>) -> Self {
        Self::Rgba8(value)
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
    /// Optional color palette for palette-indexed images (Photometric=3).
    /// Each entry is `(red, green, blue)` in the TIFF-standard 16-bit range (0..=65535).
    /// Length must be `1 << bits_per_sample` (e.g. 256 entries for an 8-bit U8 index grid).
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
            .filter_map(|(_, data)| {
                let key_value = ascii_tag_to_string(data);
                key_value
                    .split_once('=')
                    .map(|(key, value)| (key.to_owned(), value.to_owned()))
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RasterCollection {
    pub layers: Vec<Layer>,
    pub datum: Geo,
    pub shift: Pose,
    pub resolution: f64,
}

impl RasterCollection {
    pub fn get_global_properties_from_first_layer(&self) -> HashMap<String, String> {
        self.layers
            .first()
            .map(Layer::get_global_properties)
            .unwrap_or_default()
    }

    pub fn set_global_properties_on_all_layers(&mut self, properties: &HashMap<String, String>) {
        for layer in &mut self.layers {
            for (key, value) in properties {
                layer.set_global_property(key, value);
            }
        }
    }
}

pub fn global_property_tag(key: &str) -> u16 {
    let hash = key.as_bytes().iter().fold(0u32, |acc, byte| {
        acc.wrapping_mul(16777619) ^ u32::from(*byte)
    });
    GLOBAL_PROPERTIES_BASE_TAG + (hash % 1000) as u16
}

pub fn read_layer_collection<T>(path: impl AsRef<std::path::Path>) -> crate::Result<SpatialLayer<T>>
where
    T: Copy + Default + From<u8>,
{
    let collection = crate::read_raster_collection(path)?;
    if collection.layers.is_empty() {
        return Err(crate::Error::Message("No layers found in file".to_owned()));
    }

    let rows = collection.layers[0].height();
    let cols = collection.layers[0].width();
    let layers = collection.layers.len();
    let resolution = collection.resolution.max(collection.layers[0].resolution);
    let pose = collection.shift;
    let mut volume = SpatialLayer {
        rows,
        cols,
        layers,
        resolution,
        layer_height: 1.0,
        centered: true,
        pose,
        data: Vector::from_elem(rows * cols * layers, T::default()),
    };

    for (layer_idx, layer) in collection.layers.iter().enumerate() {
        let grid = match &layer.grid {
            GridData::U8(grid) => grid,
            _ => {
                return Err(crate::Error::Unsupported {
                    feature: "read_layer_collection currently supports only Grid<u8>".to_owned(),
                });
            }
        };
        for row in 0..rows {
            for col in 0..cols {
                let index = volume.index(row, col, layer_idx);
                volume.data[index] = T::from(grid[(row, col)]);
            }
        }
    }

    Ok(volume)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use datapod::{Pose, Vector};

    use super::{
        GridData, Layer, PhotometricInterpretation, RasterCollection, SampleFormat,
        ascii_tag_to_string, global_property_tag, read_layer_collection, string_to_ascii_tag,
    };
    use crate::color::Rgba8;

    fn grid_u8(rows: usize, cols: usize) -> datapod::Grid<u8> {
        datapod::Grid {
            rows,
            cols,
            resolution: 2.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from_elem(rows * cols, 0u8),
        }
    }

    #[test]
    fn ascii_tags_round_trip() {
        let encoded = string_to_ascii_tag("layer=name");
        assert_eq!(ascii_tag_to_string(&encoded), "layer=name");
    }

    #[test]
    fn grid_data_reports_expected_metadata() {
        let data = GridData::from(grid_u8(3, 4));
        assert_eq!(data.dimensions(), (3, 4));
        assert_eq!(data.bits_per_sample(), 8);
        assert_eq!(data.sample_format(), SampleFormat::UnsignedInt);
        assert_eq!(
            data.photometric_interpretation(),
            PhotometricInterpretation::BlackIsZero
        );
    }

    #[test]
    fn rgba_grid_reports_color_shape() {
        let grid = datapod::Grid {
            rows: 2,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from_elem(4, Rgba8::new(1, 2, 3, 4)),
        };
        let data = GridData::from(grid);

        assert!(data.is_color());
        assert_eq!(data.samples_per_pixel(), 4);
    }

    #[test]
    fn layer_derives_shape_from_grid() {
        let layer = Layer::new(GridData::from(grid_u8(5, 7)));
        assert_eq!(layer.height(), 5);
        assert_eq!(layer.width(), 7);
        assert_eq!(layer.samples_per_pixel(), 1);
    }

    #[test]
    fn layer_global_properties_round_trip() {
        let mut layer = Layer::new(GridData::from(grid_u8(2, 2)));
        layer.set_global_property("name", "terrain");
        assert_eq!(
            layer.get_global_properties().get("name"),
            Some(&"terrain".to_owned())
        );
    }

    #[test]
    fn collection_propagates_global_properties() {
        let mut collection = RasterCollection {
            layers: vec![
                Layer::new(GridData::from(grid_u8(1, 1))),
                Layer::new(GridData::from(grid_u8(1, 1))),
            ],
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

    #[test]
    fn read_layer_collection_stacks_u8_layers() {
        let path = std::env::temp_dir().join("rastera_read_layer_collection.tif");

        let mut collection = RasterCollection::default();
        collection.resolution = 1.0;
        let mut layer1 = Layer::new(GridData::from(datapod::Grid {
            rows: 1,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![1u8, 2]),
        }));
        let layer2 = Layer::new(GridData::from(datapod::Grid {
            rows: 1,
            cols: 2,
            resolution: 1.0,
            centered: true,
            pose: Pose::default(),
            data: Vector::from(vec![3u8, 4]),
        }));
        layer1.resolution = 1.0;
        collection.layers.push(layer1);
        collection.layers.push(layer2);

        crate::write_raster_collection(&collection, &path, &crate::WriteOptions::default())
            .unwrap();
        let volume = read_layer_collection::<u8>(&path).unwrap();
        fs::remove_file(&path).ok();

        assert_eq!(volume.layers, 2);
        assert_eq!(volume[(0, 0, 0)], 1);
        assert_eq!(volume[(0, 1, 1)], 4);
    }
}
