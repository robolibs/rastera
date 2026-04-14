//! Palette-indexed GeoTIFF example (Photometric=3, ColorMap tag 320).
//!
//! Writes a small U8 image where pixel values are indices into a 256-entry
//! color palette, then reads the file back and prints the decoded palette.
//!
//! Run with: `cargo run --example palette`

use std::env;

use datapod::{Geo, Grid, Pose, Vector};
use rastera::{
    GridData, Layer, RasterCollection, WriteOptions, read_raster_collection,
    write_raster_collection,
};

fn main() -> rastera::Result<()> {
    let rows = 4;
    let cols = 4;
    let grid = Grid {
        rows,
        cols,
        resolution: 1.0,
        centered: true,
        pose: Pose::default(),
        data: Vector::from(vec![
            0u8, 1, 2, 3,
            1, 2, 3, 0,
            2, 3, 0, 1,
            3, 0, 1, 2,
        ]),
    };

    let mut palette = vec![(0u16, 0u16, 0u16); 256];
    palette[0] = (0, 0, 0);
    palette[1] = (65535, 0, 0);
    palette[2] = (0, 65535, 0);
    palette[3] = (0, 0, 65535);

    let mut layer = Layer::new(GridData::from(grid));
    layer.datum = Geo::default();
    layer.palette = Some(palette);

    let collection = RasterCollection {
        layers: vec![layer],
        datum: Geo::default(),
        shift: Pose::default(),
        resolution: 1.0,
    };

    let path = env::temp_dir().join("rastera_palette_example.tif");
    write_raster_collection(&collection, &path, &WriteOptions::default())?;
    println!("wrote palette-indexed raster to {}", path.display());

    let parsed = read_raster_collection(&path)?;
    let layer = &parsed.layers[0];
    match &layer.grid {
        GridData::U8(g) => {
            println!("indices:");
            for r in 0..g.rows {
                let row_indices: Vec<_> = (0..g.cols).map(|c| g[(r, c)]).collect();
                println!("  {row_indices:?}");
            }
        }
        _ => unreachable!("wrote u8"),
    }
    if let Some(palette) = layer.palette.as_ref() {
        println!("palette[0..4] = {:?}", &palette[..4]);
    }

    std::fs::remove_file(&path).ok();
    Ok(())
}
