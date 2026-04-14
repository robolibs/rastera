//! Rust port of the `rastkit` raster library.
//!
//! The current implementation establishes the foundational data model used by
//! the planned TIFF and GeoTIFF parser/writer layers.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod color;
pub mod error;
pub mod ffi;
pub mod parser;
#[cfg(feature = "python")]
pub mod python;
pub mod raster;
pub mod tags;
pub mod types;
pub mod writer;

pub use color::{Rgba8, rgba_to_luma_u8};
pub use error::{Error, Result};
pub use parser::{ReadRasterCollection, read_raster_collection};
pub use raster::{GridLayer, Raster};
pub use tags::{
    GLOBAL_PROPERTIES_BASE_TAG, PRIVATE_TAG_MIN, RASTERA_RESERVED_MAX, RASTERA_RESERVED_MIN,
    TIFF_RESERVED_MAX, compression_name, is_reserved_tiff_tag, is_valid_custom_tag, tag_name,
    validate_custom_tag,
};
pub use types::{
    GridData, Layer, PhotometricInterpretation, RasterCollection, SampleFormat,
    ascii_tag_to_string, string_to_ascii_tag,
};
pub use writer::{
    WriteOptions, WriteRasterCollection, to_tiff_bytes, toTiffBytes, write_raster_collection,
};
