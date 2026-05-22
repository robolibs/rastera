use std::collections::HashMap;
use std::path::Path;

use datapod::{Encoding, Geo, Grid, Pose};

use crate::color::rgba_to_luma_u8;
use crate::error::{Error, Result};
use crate::types::{GridData, Layer, RasterCollection, make_grid};
use crate::writer::{WriteOptions, write_raster_collection};

/// A u8 grid layer: wraps a single `datapod::Grid` (which owns its data).
#[derive(Debug, Clone, PartialEq)]
pub struct GridLayer {
    pub grid: Grid,
    pub name: String,
    pub grid_type: String,
    pub properties: HashMap<String, String>,
    pub custom_tags: std::collections::BTreeMap<u16, Vec<u32>>,
}

impl GridLayer {
    pub fn new(
        grid: Grid,
        name: impl Into<String>,
        grid_type: impl Into<String>,
        properties: HashMap<String, String>,
    ) -> Self {
        Self {
            grid,
            name: name.into(),
            grid_type: grid_type.into(),
            properties,
            custom_tags: Default::default(),
        }
    }

    pub fn get(&self, row: usize, col: usize) -> u8 {
        self.grid.data[self.grid.flat_index(row, col)]
    }

    pub fn set(&mut self, row: usize, col: usize, value: u8) {
        let idx = self.grid.flat_index(row, col);
        self.grid.data[idx] = value;
    }

    pub fn from_grid_data(
        grid: &GridData,
        name: impl Into<String>,
        grid_type: impl Into<String>,
        properties: HashMap<String, String>,
    ) -> Self {
        let g = match grid {
            GridData::U8(g) => g.clone(),
            GridData::I8(g) => convert_grid_typed::<i8>(g, |v| v.clamp(0, i8::MAX) as u8),
            GridData::U16(g) => convert_grid_typed::<u16>(g, |v| v.min(255) as u8),
            GridData::I16(g) => convert_grid_typed::<i16>(g, |v| v.clamp(0, 255) as u8),
            GridData::U32(g) => convert_grid_typed::<u32>(g, |v| v.min(255) as u8),
            GridData::I32(g) => convert_grid_typed::<i32>(g, |v| v.clamp(0, 255) as u8),
            GridData::F32(g) => convert_grid_typed::<f32>(g, |v| v.clamp(0.0, 255.0) as u8),
            GridData::F64(g) => convert_grid_typed::<f64>(g, |v| v.clamp(0.0, 255.0) as u8),
            GridData::Rgba8(g) => convert_grid_typed::<crate::color::Rgba8>(g, rgba_to_luma_u8),
        };
        Self::new(g, name, grid_type, properties)
    }
}

fn convert_grid_typed<T>(g: &Grid, map: impl Fn(T) -> u8) -> Grid
where
    T: Copy + bytemuck::Pod,
{
    let typed: &[T] = bytemuck::cast_slice(&g.data);
    let bytes: Vec<u8> = typed.iter().map(|v| map(*v)).collect();
    Grid {
        rows: g.rows,
        cols: g.cols,
        encoding: Encoding::U8,
        centered: g.centered,
        resolution: g.resolution,
        pose: g.pose,
        data: bytes,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    grid_layers: Vec<GridLayer>,
    datum: Geo,
    shift: Pose,
    resolution: f64,
}

impl Raster {
    pub fn new(datum: Geo, shift: Pose, resolution: f64) -> Self {
        Self {
            grid_layers: Vec::new(),
            datum,
            shift,
            resolution,
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let collection = crate::read_raster_collection(path)?;
        let mut raster = Self::new(collection.datum, collection.shift, collection.resolution);
        for (index, layer) in collection.layers.iter().enumerate() {
            let properties = layer.get_global_properties();
            let name = properties
                .get("name")
                .cloned()
                .unwrap_or_else(|| format!("layer_{index}"));
            let grid_type = properties.get("type").cloned().unwrap_or_default();
            let mut grid_layer =
                GridLayer::from_grid_data(&layer.grid, name, grid_type, properties);
            grid_layer.custom_tags = layer.custom_tags.clone();
            raster.grid_layers.push(grid_layer);
        }
        Ok(raster)
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut collection = RasterCollection {
            layers: Vec::with_capacity(self.grid_layers.len()),
            datum: self.datum,
            shift: self.shift,
            resolution: self.resolution,
        };

        for grid_layer in &self.grid_layers {
            let mut layer = Layer::new(GridData::U8(grid_layer.grid.clone()));
            layer.datum = self.datum;
            layer.shift = self.shift;
            layer.resolution = self.resolution;
            layer.custom_tags = grid_layer.custom_tags.clone();
            for (key, value) in &grid_layer.properties {
                layer.set_global_property(key, value);
            }
            layer.set_global_property("name", &grid_layer.name);
            if !grid_layer.grid_type.is_empty() {
                layer.set_global_property("type", &grid_layer.grid_type);
            }
            collection.layers.push(layer);
        }

        write_raster_collection(&collection, path, &WriteOptions::default())
    }

    pub fn grid_count(&self) -> usize {
        self.grid_layers.len()
    }

    pub fn has_grids(&self) -> bool {
        !self.grid_layers.is_empty()
    }

    pub fn clear_grids(&mut self) {
        self.grid_layers.clear();
    }

    pub fn get_grid(&self, index: usize) -> Result<&GridLayer> {
        self.grid_layers
            .get(index)
            .ok_or_else(|| Error::Message(format!("Grid index {index} out of range")))
    }

    pub fn get_grid_mut(&mut self, index: usize) -> Result<&mut GridLayer> {
        self.grid_layers
            .get_mut(index)
            .ok_or_else(|| Error::Message(format!("Grid index {index} out of range")))
    }

    pub fn get_grid_by_name(&self, name: &str) -> Result<&GridLayer> {
        self.grid_layers
            .iter()
            .find(|layer| layer.name == name)
            .ok_or_else(|| Error::Message(format!("Grid with name '{name}' not found")))
    }

    pub fn get_grid_by_name_mut(&mut self, name: &str) -> Result<&mut GridLayer> {
        self.grid_layers
            .iter_mut()
            .find(|layer| layer.name == name)
            .ok_or_else(|| Error::Message(format!("Grid with name '{name}' not found")))
    }

    pub fn add_grid(
        &mut self,
        width: usize,
        height: usize,
        name: impl Into<String>,
        grid_type: impl Into<String>,
        mut properties: HashMap<String, String>,
    ) {
        let grid_type = grid_type.into();
        if !grid_type.is_empty() {
            properties.insert("type".to_owned(), grid_type.clone());
        }
        let grid = make_grid(
            height as u32,
            width as u32,
            Encoding::U8,
            self.resolution,
            true,
            self.shift,
            vec![0u8; width * height],
        );
        self.grid_layers
            .push(GridLayer::new(grid, name, grid_type, properties));
    }

    pub fn add_terrain_grid(&mut self, width: usize, height: usize, name: impl Into<String>) {
        self.add_grid(width, height, name, "terrain", HashMap::new());
    }

    pub fn add_occlusion_grid(&mut self, width: usize, height: usize, name: impl Into<String>) {
        self.add_grid(width, height, name, "occlusion", HashMap::new());
    }

    pub fn add_elevation_grid(&mut self, width: usize, height: usize, name: impl Into<String>) {
        self.add_grid(width, height, name, "elevation", HashMap::new());
    }

    pub fn remove_grid(&mut self, index: usize) {
        if index < self.grid_layers.len() {
            self.grid_layers.remove(index);
        }
    }

    pub fn get_grids_by_type(&self, grid_type: &str) -> Vec<&GridLayer> {
        self.grid_layers
            .iter()
            .filter(|layer| layer.grid_type == grid_type)
            .collect()
    }

    pub fn filter_by_property(&self, key: &str, value: &str) -> Vec<&GridLayer> {
        self.grid_layers
            .iter()
            .filter(|layer| {
                layer
                    .properties
                    .get(key)
                    .is_some_and(|current| current == value)
            })
            .collect()
    }

    pub fn get_grid_names(&self) -> Vec<String> {
        self.grid_layers
            .iter()
            .map(|layer| layer.name.clone())
            .collect()
    }

    pub fn datum(&self) -> Geo {
        self.datum
    }

    pub fn set_datum(&mut self, datum: Geo) {
        self.datum = datum;
    }

    pub fn shift(&self) -> Pose {
        self.shift
    }

    pub fn set_shift(&mut self, shift: Pose) {
        self.shift = shift;
    }

    pub fn resolution(&self) -> f64 {
        self.resolution
    }

    pub fn set_resolution(&mut self, resolution: f64) {
        self.resolution = resolution;
    }

    pub fn set_global_property(&mut self, key: &str, value: &str) {
        for layer in &mut self.grid_layers {
            layer.properties.insert(key.to_owned(), value.to_owned());
        }
    }

    pub fn get_global_property(&self, key: &str, default: &str) -> String {
        self.grid_layers
            .first()
            .and_then(|layer| layer.properties.get(key))
            .cloned()
            .unwrap_or_else(|| default.to_owned())
    }

    pub fn get_global_properties(&self) -> HashMap<String, String> {
        self.grid_layers
            .first()
            .map(|layer| layer.properties.clone())
            .unwrap_or_default()
    }

    pub fn remove_global_property(&mut self, key: &str) {
        for layer in &mut self.grid_layers {
            layer.properties.remove(key);
        }
    }
}

impl Default for Raster {
    fn default() -> Self {
        Self::new(Geo::new(0.001, 0.001, 1.0), Pose::default(), 1.0)
    }
}

impl<'a> IntoIterator for &'a Raster {
    type Item = &'a GridLayer;
    type IntoIter = std::slice::Iter<'a, GridLayer>;

    fn into_iter(self) -> Self::IntoIter {
        self.grid_layers.iter()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::Raster;
    use datapod::{Geo, Point, Pose, Quaternion};

    #[test]
    fn raster_file_round_trip_preserves_named_layers() {
        let mut raster = Raster::new(
            Geo::new(52.0, 5.0, 10.0),
            Pose {
                point: Point::new(1.0, 2.0, 0.0),
                rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
            },
            2.0,
        );
        raster.add_terrain_grid(3, 2, "terrain");
        raster.add_occlusion_grid(3, 2, "occlusion");
        raster.set_global_property("mission", "alpha");

        {
            let gl = raster.get_grid_mut(0).unwrap();
            gl.set(0, 0, 10);
            gl.set(1, 2, 99);
        }

        let path = std::env::temp_dir().join("rastera_raster_roundtrip.tif");
        raster.to_file(&path).unwrap();
        let loaded = Raster::from_file(&path).unwrap();
        fs::remove_file(&path).ok();

        assert_eq!(loaded.grid_count(), 2);
        assert_eq!(loaded.get_grid_by_name("terrain").unwrap().get(1, 2), 99);
        assert_eq!(loaded.get_global_property("mission", ""), "alpha");
    }
}
