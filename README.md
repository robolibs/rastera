# rastera

`rastera` is the Rust port of `../rastkit`, following the same translation style used for `../datapod_rs` and `../concord_rs`.

This crate will use local path dependencies on [`../datapod_rs`](../datapod_rs) and [`../concord_rs`](../concord_rs) during the port. For color handling, the initial plan is to use the lightweight `rgb` crate instead of the C++ `pigment` dependency.

The immediate goal is to lock down the crate shape and the migration plan before porting the GeoTIFF parser, writer, type system, and high-level raster API.

Planned conversion order:

- low-level TIFF and GeoTIFF tags, errors, and binary helpers
- `Layer` and `RasterCollection` with typed grid support backed by `datapod`
- parser and writer round-trip support
- the ergonomic `Raster` wrapper and convenience APIs
- examples and test coverage mirroring `../rastkit/test`

The implementation roadmap lives in [`PLAN.md`](PLAN.md).
