//! C ABI for rastera.
//!
//! Conventions: opaque Box-backed handles (free with the matching
//! *_free); fallible calls return bool/int with the reason in the
//! thread-local rastera_last_error_message(); borrowed views are valid
//! only for the lifetime documented by the handle they came from.
//!
//! `include/rastera.h` is generated from this file by cbindgen.

// extern "C" fns take raw pointers from C and deref them by design.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char};
use std::ptr;

use datapod::{Geo, Point, Pose, Quaternion};

use crate::color::Rgba8;
use crate::{GridData, Raster, RasterCollection, WriteOptions};

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasteraGeo3 {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasteraPoint3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasteraQuat {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasteraPose {
    pub point: RasteraPoint3,
    pub rotation: RasteraQuat,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasteraRgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub struct RasteraRaster {
    inner: Raster,
}

impl From<RasteraGeo3> for Geo {
    fn from(value: RasteraGeo3) -> Self {
        Self::new(value.latitude, value.longitude, value.altitude)
    }
}

impl From<Geo> for RasteraGeo3 {
    fn from(value: Geo) -> Self {
        Self {
            latitude: value.latitude,
            longitude: value.longitude,
            altitude: value.altitude,
        }
    }
}

impl From<RasteraPoint3> for Point {
    fn from(value: RasteraPoint3) -> Self {
        Self::new(value.x, value.y, value.z)
    }
}

impl From<Point> for RasteraPoint3 {
    fn from(value: Point) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

impl From<RasteraQuat> for Quaternion {
    fn from(value: RasteraQuat) -> Self {
        Self::new(value.w, value.x, value.y, value.z)
    }
}

impl From<Quaternion> for RasteraQuat {
    fn from(value: Quaternion) -> Self {
        Self {
            w: value.w,
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

impl From<RasteraPose> for Pose {
    fn from(value: RasteraPose) -> Self {
        Self {
            point: value.point.into(),
            rotation: value.rotation.into(),
        }
    }
}

impl From<Pose> for RasteraPose {
    fn from(value: Pose) -> Self {
        Self {
            point: value.point.into(),
            rotation: value.rotation.into(),
        }
    }
}

fn clear_last_error() {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

fn set_last_error(message: impl Into<String>) {
    let message = message.into().replace('\0', " ");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = Some(
            CString::new(message).unwrap_or_else(|_| CString::new("rastera ffi error").unwrap()),
        );
    });
}

fn ok() -> bool {
    clear_last_error();
    true
}

fn fail(message: impl Into<String>) -> bool {
    set_last_error(message);
    false
}

fn cstr_to_str<'a>(value: *const c_char, label: &str) -> crate::Result<&'a str> {
    if value.is_null() {
        return Err(crate::Error::Message(format!("null {label} pointer")));
    }
    let cstr = unsafe { CStr::from_ptr(value) };
    cstr.to_str()
        .map_err(|_| crate::Error::Message(format!("{label} must be valid UTF-8")))
}

fn raster_from_ptr_mut<'a>(handle: *mut RasteraRaster) -> crate::Result<&'a mut RasteraRaster> {
    if handle.is_null() {
        return Err(crate::Error::Message("null raster handle".into()));
    }
    Ok(unsafe { &mut *handle })
}

fn raster_from_ptr<'a>(handle: *const RasteraRaster) -> crate::Result<&'a RasteraRaster> {
    if handle.is_null() {
        return Err(crate::Error::Message("null raster handle".into()));
    }
    Ok(unsafe { &*handle })
}

fn write_out<T>(out: *mut T, value: T) -> bool {
    if out.is_null() {
        return fail("null output pointer");
    }
    unsafe { ptr::write(out, value) };
    ok()
}

fn string_to_ptr(value: String) -> *mut c_char {
    CString::new(value)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(ptr::null(), |message| message.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

// ----- Raster lifecycle --------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_new(
    datum: RasteraGeo3,
    shift: RasteraPose,
    resolution: f64,
) -> *mut RasteraRaster {
    clear_last_error();
    Box::into_raw(Box::new(RasteraRaster {
        inner: Raster::new(datum.into(), shift.into(), resolution),
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_from_file(path: *const c_char) -> *mut RasteraRaster {
    clear_last_error();
    match cstr_to_str(path, "path").and_then(Raster::from_file) {
        Ok(inner) => Box::into_raw(Box::new(RasteraRaster { inner })),
        Err(err) => {
            set_last_error(err.to_string());
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_to_file(
    handle: *const RasteraRaster,
    path: *const c_char,
) -> bool {
    match raster_from_ptr(handle)
        .and_then(|handle| cstr_to_str(path, "path").and_then(|path| handle.inner.to_file(path)))
    {
        Ok(()) => ok(),
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_free(handle: *mut RasteraRaster) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}

// ----- Raster metadata ---------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_get_datum(
    handle: *const RasteraRaster,
    out: *mut RasteraGeo3,
) -> bool {
    match raster_from_ptr(handle) {
        Ok(handle) => write_out(out, handle.inner.datum().into()),
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_set_datum(handle: *mut RasteraRaster, datum: RasteraGeo3) -> bool {
    match raster_from_ptr_mut(handle) {
        Ok(handle) => {
            handle.inner.set_datum(datum.into());
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_get_shift(
    handle: *const RasteraRaster,
    out: *mut RasteraPose,
) -> bool {
    match raster_from_ptr(handle) {
        Ok(handle) => write_out(out, handle.inner.shift().into()),
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_set_shift(handle: *mut RasteraRaster, shift: RasteraPose) -> bool {
    match raster_from_ptr_mut(handle) {
        Ok(handle) => {
            handle.inner.set_shift(shift.into());
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_get_resolution(handle: *const RasteraRaster) -> f64 {
    match raster_from_ptr(handle) {
        Ok(handle) => handle.inner.resolution(),
        Err(err) => {
            set_last_error(err.to_string());
            f64::NAN
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_set_resolution(
    handle: *mut RasteraRaster,
    resolution: f64,
) -> bool {
    match raster_from_ptr_mut(handle) {
        Ok(handle) => {
            handle.inner.set_resolution(resolution);
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_set_global_property(
    handle: *mut RasteraRaster,
    key: *const c_char,
    value: *const c_char,
) -> bool {
    match raster_from_ptr_mut(handle).and_then(|handle| {
        Ok((
            handle,
            cstr_to_str(key, "key")?.to_string(),
            cstr_to_str(value, "value")?.to_string(),
        ))
    }) {
        Ok((handle, key, value)) => {
            handle.inner.set_global_property(&key, &value);
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_get_global_property(
    handle: *const RasteraRaster,
    key: *const c_char,
) -> *mut c_char {
    clear_last_error();
    match raster_from_ptr(handle).and_then(|handle| {
        let key = cstr_to_str(key, "key")?;
        Ok(handle.inner.get_global_property(key, ""))
    }) {
        Ok(value) => string_to_ptr(value),
        Err(err) => {
            set_last_error(err.to_string());
            ptr::null_mut()
        }
    }
}

// ----- Grid layer management ---------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_count(handle: *const RasteraRaster) -> usize {
    raster_from_ptr(handle)
        .map(|h| h.inner.grid_count())
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_add_grid(
    handle: *mut RasteraRaster,
    width: usize,
    height: usize,
    name: *const c_char,
    grid_type: *const c_char,
) -> bool {
    match raster_from_ptr_mut(handle).and_then(|handle| {
        let name = cstr_to_str(name, "name")?.to_string();
        let grid_type = if grid_type.is_null() {
            String::new()
        } else {
            cstr_to_str(grid_type, "grid_type")?.to_string()
        };
        handle
            .inner
            .add_grid(width, height, name, grid_type, Default::default());
        Ok(())
    }) {
        Ok(()) => ok(),
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_remove_grid(handle: *mut RasteraRaster, index: usize) -> bool {
    match raster_from_ptr_mut(handle) {
        Ok(handle) => {
            handle.inner.remove_grid(index);
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_name(
    handle: *const RasteraRaster,
    index: usize,
) -> *mut c_char {
    clear_last_error();
    match raster_from_ptr(handle).and_then(|h| h.inner.get_grid(index)) {
        Ok(grid) => string_to_ptr(grid.name.clone()),
        Err(err) => {
            set_last_error(err.to_string());
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_type(
    handle: *const RasteraRaster,
    index: usize,
) -> *mut c_char {
    clear_last_error();
    match raster_from_ptr(handle).and_then(|h| h.inner.get_grid(index)) {
        Ok(grid) => string_to_ptr(grid.grid_type.clone()),
        Err(err) => {
            set_last_error(err.to_string());
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_shape(
    handle: *const RasteraRaster,
    index: usize,
    out_rows: *mut usize,
    out_cols: *mut usize,
) -> bool {
    match raster_from_ptr(handle).and_then(|h| h.inner.get_grid(index)) {
        Ok(grid) => {
            if out_rows.is_null() || out_cols.is_null() {
                return fail("null output pointer");
            }
            unsafe {
                ptr::write(out_rows, grid.grid.rows as usize);
                ptr::write(out_cols, grid.grid.cols as usize);
            }
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_get_u8(
    handle: *const RasteraRaster,
    index: usize,
    row: usize,
    col: usize,
    out: *mut u8,
) -> bool {
    match raster_from_ptr(handle).and_then(|h| h.inner.get_grid(index)) {
        Ok(grid) => {
            if row >= grid.grid.rows as usize || col >= grid.grid.cols as usize {
                return fail("grid index out of bounds");
            }
            write_out(out, grid.get(row, col))
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_set_u8(
    handle: *mut RasteraRaster,
    index: usize,
    row: usize,
    col: usize,
    value: u8,
) -> bool {
    match raster_from_ptr_mut(handle).and_then(|h| h.inner.get_grid_mut(index)) {
        Ok(grid) => {
            if row >= grid.grid.rows as usize || col >= grid.grid.cols as usize {
                return fail("grid index out of bounds");
            }
            grid.set(row, col, value);
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_fill_u8(
    handle: *mut RasteraRaster,
    index: usize,
    value: u8,
) -> bool {
    match raster_from_ptr_mut(handle).and_then(|h| h.inner.get_grid_mut(index)) {
        Ok(grid) => {
            for v in &mut grid.grid.data {
                *v = value;
            }
            ok()
        }
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_raster_grid_find_by_name(
    handle: *const RasteraRaster,
    name: *const c_char,
    out_index: *mut usize,
) -> bool {
    match raster_from_ptr(handle).and_then(|h| {
        let name = cstr_to_str(name, "name")?;
        for i in 0..h.inner.grid_count() {
            if h.inner.get_grid(i)?.name == name {
                return Ok(i);
            }
        }
        Err(crate::Error::Message(format!(
            "grid named '{name}' not found"
        )))
    }) {
        Ok(index) => write_out(out_index, index),
        Err(err) => fail(err.to_string()),
    }
}

// ----- RGBA write helper (creates a standalone raster file) -------------

#[unsafe(no_mangle)]
pub extern "C" fn rastera_write_rgba8(
    path: *const c_char,
    rows: usize,
    cols: usize,
    pixels: *const RasteraRgba,
    datum: RasteraGeo3,
    resolution: f64,
) -> bool {
    if pixels.is_null() {
        return fail("null pixels pointer");
    }
    if rows == 0 || cols == 0 {
        return fail("rows and cols must be non-zero");
    }
    let path_str = match cstr_to_str(path, "path") {
        Ok(s) => s,
        Err(err) => return fail(err.to_string()),
    };
    let cells = rows * cols;
    let slice = unsafe { std::slice::from_raw_parts(pixels, cells) };
    let pixels: Vec<Rgba8> = slice
        .iter()
        .map(|p| Rgba8::new(p.r, p.g, p.b, p.a))
        .collect();
    let bytes: Vec<u8> = bytemuck::cast_slice(&pixels).to_vec();
    let grid = datapod::Grid {
        rows: rows as u32,
        cols: cols as u32,
        encoding: datapod::Encoding::Rgba8,
        centered: 1,
        resolution,
        pose: Pose::default(),
        data: bytes,
    };
    let mut layer = crate::Layer::new(GridData::Rgba8(grid));
    layer.datum = datum.into();
    layer.resolution = resolution;
    let collection = RasterCollection {
        layers: vec![layer],
        datum: datum.into(),
        shift: Pose::default(),
        resolution,
    };
    match crate::write_raster_collection(&collection, path_str, &WriteOptions::default()) {
        Ok(()) => ok(),
        Err(err) => fail(err.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rastera_read_rgba8_size(
    path: *const c_char,
    out_rows: *mut usize,
    out_cols: *mut usize,
) -> bool {
    let path_str = match cstr_to_str(path, "path") {
        Ok(s) => s,
        Err(err) => return fail(err.to_string()),
    };
    let collection = match crate::read_raster_collection(path_str) {
        Ok(c) => c,
        Err(err) => return fail(err.to_string()),
    };
    let Some(layer) = collection.layers.first() else {
        return fail("no layers in file");
    };
    let (rows, cols) = layer.grid.dimensions();
    if out_rows.is_null() || out_cols.is_null() {
        return fail("null output pointer");
    }
    unsafe {
        ptr::write(out_rows, rows);
        ptr::write(out_cols, cols);
    }
    ok()
}

/// Read an RGBA raster into a caller-provided buffer. `capacity` must be at
/// least `rows * cols`, as reported by `rastera_read_rgba8_size`.
#[unsafe(no_mangle)]
pub extern "C" fn rastera_read_rgba8_into(
    path: *const c_char,
    out_pixels: *mut RasteraRgba,
    capacity: usize,
) -> bool {
    if out_pixels.is_null() {
        return fail("null output buffer");
    }
    let path_str = match cstr_to_str(path, "path") {
        Ok(s) => s,
        Err(err) => return fail(err.to_string()),
    };
    let collection = match crate::read_raster_collection(path_str) {
        Ok(c) => c,
        Err(err) => return fail(err.to_string()),
    };
    let Some(layer) = collection.layers.first() else {
        return fail("no layers in file");
    };
    let grid = match &layer.grid {
        GridData::Rgba8(g) => g,
        _ => return fail("first layer is not an RGBA grid"),
    };
    let needed = (grid.rows as usize) * (grid.cols as usize);
    if capacity < needed {
        return fail(format!(
            "buffer capacity {capacity} is smaller than required {needed}"
        ));
    }
    let pixels: &[Rgba8] = bytemuck::cast_slice(&grid.data);
    let slice = unsafe { std::slice::from_raw_parts_mut(out_pixels, needed) };
    for (dst, src) in slice.iter_mut().zip(pixels.iter()) {
        *dst = RasteraRgba {
            r: src.r,
            g: src.g,
            b: src.b,
            a: src.a,
        };
    }
    ok()
}
