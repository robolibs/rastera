//! Write and read back an RGBA GeoTIFF.
//!
//! Builds a 64x64 "test card" image with four color quadrants, writes it,
//! and reads the image back to verify the pixels round-trip.
//!
//! Run with: `cargo run --example rgba_image`

use std::env;

use datapod::{Encoding, Geo, Grid, Pose};
use rastera::color::Rgba8;
use rastera::{
    GridData, Layer, RasterCollection, WriteOptions, read_raster_collection,
    write_raster_collection,
};

fn main() -> rastera::Result<()> {
    let rows = 64usize;
    let cols = 64usize;
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let top = r < rows / 2;
            let left = c < cols / 2;
            let px = match (top, left) {
                (true, true) => Rgba8::new(255, 0, 0, 255),
                (true, false) => Rgba8::new(0, 255, 0, 255),
                (false, true) => Rgba8::new(0, 0, 255, 255),
                (false, false) => Rgba8::new(255, 255, 0, 255),
            };
            data.push(px);
        }
    }

    let grid = Grid {
        rows: rows as u32,
        cols: cols as u32,
        encoding: Encoding::Rgba8,
        resolution: 0.5,
        centered: 1,
        pose: Pose::default(),
        data: bytemuck::cast_slice(&data).to_vec(),
    };
    let mut layer = Layer::new(GridData::from(grid));
    layer.datum = Geo::new(52.0, 5.0, 0.0);
    layer.resolution = 0.5;

    let collection = RasterCollection {
        layers: vec![layer],
        datum: Geo::new(52.0, 5.0, 0.0),
        shift: Pose::default(),
        resolution: 0.5,
    };

    let path = env::temp_dir().join("rastera_rgba_example.tif");
    write_raster_collection(&collection, &path, &WriteOptions::default())?;
    println!("wrote {}", path.display());

    let parsed = read_raster_collection(&path)?;
    match &parsed.layers[0].grid {
        GridData::Rgba8(g) => {
            let pixels: &[Rgba8] = bytemuck::cast_slice(&g.data);
            println!("read {} x {} RGBA grid", g.rows, g.cols);
            println!("top-left    px = {:?}", pixels[g.flat_index(0, 0)]);
            println!("top-right   px = {:?}", pixels[g.flat_index(0, cols - 1)]);
            println!("bottom-left px = {:?}", pixels[g.flat_index(rows - 1, 0)]);
            println!(
                "bottom-right px = {:?}",
                pixels[g.flat_index(rows - 1, cols - 1)]
            );
        }
        _ => unreachable!("written as RGBA"),
    }

    std::fs::remove_file(&path).ok();
    Ok(())
}
