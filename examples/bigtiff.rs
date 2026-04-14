//! BigTIFF and big-endian output example.
//!
//! Writes the same i16 raster twice — once as classic little-endian TIFF and
//! once as a forced BigTIFF with big-endian byte order — then reads both back
//! and confirms the pixel data matches.
//!
//! Run with: `cargo run --example bigtiff`

use std::env;

use datapod::{Geo, Grid, Pose, Vector};
use rastera::{
    GridData, Layer, RasterCollection, WriteOptions, read_raster_collection,
    write_raster_collection,
};

fn main() -> rastera::Result<()> {
    let rows = 6;
    let cols = 8;
    let data: Vec<i16> = (0..(rows * cols))
        .map(|i| (i as i16) * 100 - 1000)
        .collect();
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

    let le_path = env::temp_dir().join("rastera_classic_le.tif");
    write_raster_collection(&collection, &le_path, &WriteOptions::default())?;
    println!("classic LE  -> {}", le_path.display());

    let be_bigtiff_options = WriteOptions {
        big_endian: true,
        force_bigtiff: true,
        ..WriteOptions::default()
    };
    let be_path = env::temp_dir().join("rastera_big_be.tif");
    write_raster_collection(&collection, &be_path, &be_bigtiff_options)?;
    println!("BigTIFF BE  -> {}", be_path.display());

    for (label, path) in [("classic LE", &le_path), ("BigTIFF BE", &be_path)] {
        let parsed = read_raster_collection(path)?;
        match &parsed.layers[0].grid {
            GridData::I16(g) => {
                assert_eq!(g.data.as_slice(), data.as_slice());
                println!(
                    "{label}: shape={}x{} first={} last={}",
                    g.rows,
                    g.cols,
                    g.data[0],
                    g.data[g.data.as_slice().len() - 1]
                );
            }
            _ => unreachable!("wrote i16"),
        }
    }

    std::fs::remove_file(&le_path).ok();
    std::fs::remove_file(&be_path).ok();
    Ok(())
}
