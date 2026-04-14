# rastera

GeoTIFF and raster I/O library for Rust, with C and Python bindings.

## Features

- Classic TIFF and **BigTIFF** (auto-detected or forced)
- **Little-endian and big-endian** output and input
- Single-strip and **multi-strip** layouts with configurable `rows_per_strip`
- Multi-IFD write/read for multi-layer rasters
- Scalar grid types: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `f32`, `f64`
- **RGBA** images (Photometric RGB + ExtraSamples) and 3-channel RGB → RGBA lift on read
- **Palette-indexed** images (Photometric=3 + ColorMap tag 320)
- Chunky and **separated-plane** (`PlanarConfiguration = 2`) layouts for multi-sample images
- Full GeoTIFF metadata:
  - `ModelPixelScaleTag` / `ModelTiepointTag` for un-rotated grids
  - `ModelTransformationTag` for rotated grids (yaw from quaternion)
  - `GeoKeyDirectoryTag` with WGS84 base keys plus conditional citation and vertical-CRS keys
  - `GeoDoubleParamsTag`, `GeoAsciiParamsTag`
  - `GDAL_NODATA`, `VerticalCitationGeoKey`, `VerticalDatumGeoKey`, `VerticalUnitsGeoKey`
- Custom/private TIFF tag round-trip and hashed global-property tags
- High-level `Raster` wrapper with layer-by-name lookup, grid iteration, and global properties
- C ABI (`rastera.h`) and Python bindings via PyO3

## Quickstart (Rust)

```rust
use datapod::{Geo, Point, Pose, Quaternion};
use rastera::Raster;

let mut raster = Raster::new(
    Geo::new(52.0, 5.0, 10.0),
    Pose {
        point: Point::new(0.0, 0.0, 0.0),
        rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
    },
    1.0,
);
raster.add_terrain_grid(16, 16, "terrain");
raster.set_global_property("mission", "alpha");

{
    let grid = &mut raster.get_grid_mut(0).unwrap().grid;
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            grid[(r, c)] = ((r + c) * 8) as u8;
        }
    }
}

raster.to_file("/tmp/example.tif")?;
let loaded = Raster::from_file("/tmp/example.tif")?;
assert_eq!(loaded.grid_count(), 1);
```

## Writer options

```rust
use rastera::WriteOptions;

let opts = WriteOptions {
    rows_per_strip: 0,      // 0 = auto (target ~8 KB strips); u32::MAX = single strip
    force_bigtiff: false,   // force BigTIFF even for small files
    big_endian: false,      // emit MM byte order
    planar_config: 1,       // 1 = chunky, 2 = separated planes (RGBA)
    software: "my-tool".to_owned(),
};
```

## Running the Rust examples

```bash
make run EXAMPLE=main          # u8 round-trip smoke test
make run EXAMPLE=rgba_image    # 64×64 RGBA test card
make run EXAMPLE=multi_layer   # multi-IFD terrain + occlusion + elevation
make run EXAMPLE=palette       # palette-indexed image with ColorMap
make run EXAMPLE=bigtiff       # classic LE and forced BigTIFF/BE
```

## C API

The cdylib ships with a hand-written header at [`include/rastera.h`](include/rastera.h). Build and run the bundled demo:

```bash
make c-demo
```

Example usage:

```c
#include "rastera.h"

RasteraGeo3 datum = {52.0, 5.0, 10.0};
RasteraPose shift = {{0, 0, 0}, {1, 0, 0, 0}};

RasteraRasterHandle* r = rastera_raster_new(datum, shift, 1.0);
rastera_raster_add_grid(r, 8, 8, "terrain", "terrain");
rastera_raster_grid_fill_u8(r, 0, 42);
rastera_raster_to_file(r, "/tmp/demo.tif");
rastera_raster_free(r);
```

All fallible functions return `bool` (or `NULL`) on failure; call `rastera_last_error_message()` to retrieve the thread-local error string. Strings returned by the library must be released with `rastera_string_free`.

## Python bindings

Built via [maturin](https://www.maturin.rs/) with the `python-extension` feature. A `.venv` + `maturin develop` workflow is wrapped in the example Makefile:

```bash
make python-basic   # runs examples/python_binding/basic.py
make python-rgba    # runs examples/python_binding/rgba.py
```

Example usage:

```python
import rastera

r = rastera.Raster(datum=(52.0, 5.0, 10.0), resolution=1.0)
r.add_terrain_grid(8, 8)
r.add_occlusion_grid(8, 8)

for row in range(8):
    for col in range(8):
        r.grid_set(0, row, col, (row + col) * 16)

r.to_file("/tmp/example.tif")

loaded = rastera.read_file("/tmp/example.tif")
print(loaded.grid_names(), loaded.grid_shape(0))

# Direct RGBA helper
rastera.write_rgba8(
    "/tmp/rgba.tif",
    rows=2, cols=2,
    pixels=[(255,0,0,255), (0,255,0,255), (0,0,255,255), (255,255,255,255)],
    datum=(52.0, 5.0, 0.0),
    resolution=0.5,
)
```

## Makefile targets

```
make build         Build the library and Rust examples
make test          Run the Rust test suite
make test-python   Type-check with the python feature enabled
make run EXAMPLE=  Run a Rust example
make c-demo        Build and run the C ABI demo
make python-basic  Build the wheel and run the basic Python demo
make python-rgba   Build the wheel and run the RGBA Python demo
make fmt           Format the workspace
make clean         Remove Cargo artifacts and the Python venv
```

## Project layout

```
src/
  color.rs        RGBA type alias and luma helper
  error.rs        Error / Result types
  tags.rs         TIFF tag constants and validation
  types.rs        GridData, Layer, RasterCollection
  writer.rs       TIFF / GeoTIFF writer
  parser.rs       TIFF / GeoTIFF parser
  raster.rs       High-level Raster wrapper
  ffi.rs          C ABI layer (always compiled)
  python.rs       PyO3 bindings (gated behind the `python` feature)
include/
  rastera.h       Public C header
examples/
  main.rs / rgba_image.rs / multi_layer.rs / palette.rs / bigtiff.rs
  c_abi/          C demo + Makefile
  python_binding/ Python demos + Makefile
```

## License

MIT.
