//! PyO3 bindings for rastera.
//!
//! Exposes a Python module `rastera` with:
//!
//! - `Raster` — high-level multi-layer raster with read/write/add_grid
//! - `read_file(path) -> Raster`
//! - `write_rgba8(path, rows, cols, pixels, datum, resolution)` — write a single
//!   RGBA GeoTIFF in one call
//! - `read_rgba8(path) -> (rows, cols, list of (r, g, b, a) tuples)`
//!
//! Build with `maturin develop --features python-extension`.

use std::collections::HashMap;

use datapod::{Geo, Point, Pose, Quaternion};
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use pyo3::wrap_pyfunction;

use crate::color::Rgba8;
use crate::{GridData, Layer, Raster, RasterCollection, WriteOptions};

fn py_runtime_error(err: crate::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

fn geo_from_tuple(value: (f64, f64, f64)) -> Geo {
    Geo::new(value.0, value.1, value.2)
}

fn geo_to_tuple(value: Geo) -> (f64, f64, f64) {
    (value.latitude, value.longitude, value.altitude)
}

fn pose_from_tuple(
    point: (f64, f64, f64),
    rotation: (f64, f64, f64, f64),
) -> Pose {
    Pose {
        point: Point::new(point.0, point.1, point.2),
        rotation: Quaternion::new(rotation.0, rotation.1, rotation.2, rotation.3),
    }
}

fn pose_to_tuple(value: Pose) -> ((f64, f64, f64), (f64, f64, f64, f64)) {
    (
        (value.point.x, value.point.y, value.point.z),
        (
            value.rotation.w,
            value.rotation.x,
            value.rotation.y,
            value.rotation.z,
        ),
    )
}

/// High-level multi-layer raster (Python `rastera.Raster`).
#[pyclass(name = "Raster")]
pub struct PyRaster {
    inner: Raster,
}

#[pymethods]
impl PyRaster {
    #[new]
    #[pyo3(signature = (
        datum=(0.001, 0.001, 1.0),
        shift=((0.0, 0.0, 0.0), (1.0, 0.0, 0.0, 0.0)),
        resolution=1.0,
    ))]
    fn new(
        datum: (f64, f64, f64),
        shift: ((f64, f64, f64), (f64, f64, f64, f64)),
        resolution: f64,
    ) -> Self {
        let (point, quat) = shift;
        Self {
            inner: Raster::new(geo_from_tuple(datum), pose_from_tuple(point, quat), resolution),
        }
    }

    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        Ok(Self {
            inner: Raster::from_file(path).map_err(py_runtime_error)?,
        })
    }

    fn to_file(&self, path: &str) -> PyResult<()> {
        self.inner.to_file(path).map_err(py_runtime_error)
    }

    // ----- metadata ------------------------------------------------------

    fn datum(&self) -> (f64, f64, f64) {
        geo_to_tuple(self.inner.datum())
    }

    fn set_datum(&mut self, datum: (f64, f64, f64)) {
        self.inner.set_datum(geo_from_tuple(datum));
    }

    fn shift(&self) -> ((f64, f64, f64), (f64, f64, f64, f64)) {
        pose_to_tuple(self.inner.shift())
    }

    fn set_shift(
        &mut self,
        shift: ((f64, f64, f64), (f64, f64, f64, f64)),
    ) {
        self.inner.set_shift(pose_from_tuple(shift.0, shift.1));
    }

    fn resolution(&self) -> f64 {
        self.inner.resolution()
    }

    fn set_resolution(&mut self, resolution: f64) {
        self.inner.set_resolution(resolution);
    }

    fn set_global_property(&mut self, key: &str, value: &str) {
        self.inner.set_global_property(key, value);
    }

    #[pyo3(signature = (key, default=""))]
    fn get_global_property(&self, key: &str, default: &str) -> String {
        self.inner.get_global_property(key, default)
    }

    fn get_global_properties(&self) -> HashMap<String, String> {
        self.inner.get_global_properties()
    }

    fn remove_global_property(&mut self, key: &str) {
        self.inner.remove_global_property(key);
    }

    // ----- grid layers ---------------------------------------------------

    fn grid_count(&self) -> usize {
        self.inner.grid_count()
    }

    fn grid_names(&self) -> Vec<String> {
        self.inner.get_grid_names()
    }

    #[pyo3(signature = (width, height, name, grid_type="", properties=None))]
    fn add_grid(
        &mut self,
        width: usize,
        height: usize,
        name: &str,
        grid_type: &str,
        properties: Option<HashMap<String, String>>,
    ) {
        self.inner.add_grid(
            width,
            height,
            name,
            grid_type,
            properties.unwrap_or_default(),
        );
    }

    #[pyo3(signature = (width, height, name=None))]
    fn add_terrain_grid(&mut self, width: usize, height: usize, name: Option<&str>) {
        self.inner.add_terrain_grid(width, height, name.unwrap_or("terrain"));
    }

    #[pyo3(signature = (width, height, name=None))]
    fn add_occlusion_grid(&mut self, width: usize, height: usize, name: Option<&str>) {
        self.inner
            .add_occlusion_grid(width, height, name.unwrap_or("occlusion"));
    }

    #[pyo3(signature = (width, height, name=None))]
    fn add_elevation_grid(&mut self, width: usize, height: usize, name: Option<&str>) {
        self.inner
            .add_elevation_grid(width, height, name.unwrap_or("elevation"));
    }

    fn remove_grid(&mut self, index: usize) {
        self.inner.remove_grid(index);
    }

    fn grid_shape(&self, index: usize) -> PyResult<(usize, usize)> {
        let grid = self
            .inner
            .get_grid(index)
            .map_err(|_| PyIndexError::new_err("grid index out of range"))?;
        Ok((grid.grid.rows, grid.grid.cols))
    }

    fn grid_name(&self, index: usize) -> PyResult<String> {
        let grid = self
            .inner
            .get_grid(index)
            .map_err(|_| PyIndexError::new_err("grid index out of range"))?;
        Ok(grid.name.clone())
    }

    fn grid_type(&self, index: usize) -> PyResult<String> {
        let grid = self
            .inner
            .get_grid(index)
            .map_err(|_| PyIndexError::new_err("grid index out of range"))?;
        Ok(grid.grid_type.clone())
    }

    fn grid_get(&self, index: usize, row: usize, col: usize) -> PyResult<u8> {
        let grid = self
            .inner
            .get_grid(index)
            .map_err(|_| PyIndexError::new_err("grid index out of range"))?;
        if row >= grid.grid.rows || col >= grid.grid.cols {
            return Err(PyIndexError::new_err("pixel index out of range"));
        }
        Ok(grid.grid[(row, col)])
    }

    fn grid_set(
        &mut self,
        index: usize,
        row: usize,
        col: usize,
        value: u8,
    ) -> PyResult<()> {
        let grid = self
            .inner
            .get_grid_mut(index)
            .map_err(|_| PyIndexError::new_err("grid index out of range"))?;
        if row >= grid.grid.rows || col >= grid.grid.cols {
            return Err(PyIndexError::new_err("pixel index out of range"));
        }
        grid.grid[(row, col)] = value;
        Ok(())
    }

    fn grid_fill(&mut self, index: usize, value: u8) -> PyResult<()> {
        let grid = self
            .inner
            .get_grid_mut(index)
            .map_err(|_| PyIndexError::new_err("grid index out of range"))?;
        for r in 0..grid.grid.rows {
            for c in 0..grid.grid.cols {
                grid.grid[(r, c)] = value;
            }
        }
        Ok(())
    }

    fn grid_data<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Py<PyAny>> {
        let grid = self
            .inner
            .get_grid(index)
            .map_err(|_| PyIndexError::new_err("grid index out of range"))?;
        Ok(grid.grid.data.as_slice().to_vec().into_pyobject(py)?.into_any().unbind())
    }

    fn find_grid(&self, name: &str) -> PyResult<usize> {
        for index in 0..self.inner.grid_count() {
            if self
                .inner
                .get_grid(index)
                .map(|g| g.name == name)
                .unwrap_or(false)
            {
                return Ok(index);
            }
        }
        Err(PyValueError::new_err(format!("grid '{name}' not found")))
    }
}

#[pyfunction]
fn read_file(path: &str) -> PyResult<PyRaster> {
    Ok(PyRaster {
        inner: Raster::from_file(path).map_err(py_runtime_error)?,
    })
}

#[pyfunction]
#[pyo3(signature = (path, rows, cols, pixels, datum=(0.001, 0.001, 1.0), resolution=1.0))]
fn write_rgba8(
    path: &str,
    rows: usize,
    cols: usize,
    pixels: Vec<(u8, u8, u8, u8)>,
    datum: (f64, f64, f64),
    resolution: f64,
) -> PyResult<()> {
    if rows * cols != pixels.len() {
        return Err(PyValueError::new_err(format!(
            "pixels length {} does not match rows*cols ({})",
            pixels.len(),
            rows * cols
        )));
    }
    let data: Vec<Rgba8> = pixels
        .into_iter()
        .map(|(r, g, b, a)| Rgba8::new(r, g, b, a))
        .collect();
    let grid = datapod::Grid {
        rows,
        cols,
        resolution,
        centered: true,
        pose: Pose::default(),
        data: datapod::Vector::from(data),
    };
    let mut layer = Layer::new(GridData::from(grid));
    layer.datum = geo_from_tuple(datum);
    layer.resolution = resolution;
    let collection = RasterCollection {
        layers: vec![layer],
        datum: geo_from_tuple(datum),
        shift: Pose::default(),
        resolution,
    };
    crate::write_raster_collection(&collection, path, &WriteOptions::default())
        .map_err(py_runtime_error)
}

#[pyfunction]
fn read_rgba8<'py>(py: Python<'py>, path: &str) -> PyResult<Bound<'py, PyDict>> {
    let collection = crate::read_raster_collection(path).map_err(py_runtime_error)?;
    let layer = collection
        .layers
        .first()
        .ok_or_else(|| PyRuntimeError::new_err("no layers in file"))?;
    let (rows, cols) = layer.grid.dimensions();
    let pixels: Vec<(u8, u8, u8, u8)> = match &layer.grid {
        GridData::Rgba8(g) => g.data.as_slice().iter().map(|p| (p.r, p.g, p.b, p.a)).collect(),
        _ => {
            return Err(PyRuntimeError::new_err(
                "first layer is not an RGBA grid",
            ));
        }
    };
    let out = PyDict::new(py);
    out.set_item("rows", rows)?;
    out.set_item("cols", cols)?;
    out.set_item("pixels", pixels)?;
    Ok(out)
}

#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn register_python_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(read_file, module)?)?;
    module.add_function(wrap_pyfunction!(write_rgba8, module)?)?;
    module.add_function(wrap_pyfunction!(read_rgba8, module)?)?;
    module.add_function(wrap_pyfunction!(version, module)?)?;
    module.add_class::<PyRaster>()?;
    Ok(())
}

#[pymodule]
fn rastera(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_python_module(module)
}
