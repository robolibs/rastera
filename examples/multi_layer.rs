//! Multi-layer raster example: terrain + occlusion + elevation.
//!
//! Demonstrates writing a three-layer GeoTIFF (multi-IFD), attaching
//! per-raster global properties, and reading the layers back by name.
//!
//! Run with: `cargo run --example multi_layer`

use std::env;

use datapod::{Geo, Point, Pose, Quaternion};
use rastera::Raster;

fn main() -> rastera::Result<()> {
    let mut raster = Raster::new(
        Geo::new(52.0, 5.0, 10.0),
        Pose {
            point: Point::new(3.0, 4.0, 0.0),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
        },
        2.0,
    );
    raster.add_terrain_grid(8, 8, "terrain");
    raster.add_occlusion_grid(8, 8, "occlusion");
    raster.add_elevation_grid(8, 8, "elevation");
    raster.set_global_property("mission", "alpha");
    raster.set_global_property("field", "F42");

    for (idx, base) in [10u8, 50u8, 200u8].iter().enumerate() {
        let grid = &mut raster.get_grid_mut(idx).unwrap().grid;
        for r in 0..grid.rows {
            for c in 0..grid.cols {
                grid[(r, c)] = base.wrapping_add(((r + c) as u8).wrapping_mul(3));
            }
        }
    }

    let path = env::temp_dir().join("rastera_multi_layer_example.tif");
    raster.to_file(&path)?;
    println!("wrote 3-layer raster to {}", path.display());

    let loaded = Raster::from_file(&path)?;
    println!("grid_count = {}", loaded.grid_count());
    for name in loaded.get_grid_names() {
        let grid = loaded.get_grid_by_name(&name)?.grid.clone();
        println!(
            "  {:<9} {}x{} first={:3} last={:3}",
            name,
            grid.rows,
            grid.cols,
            grid[(0, 0)],
            grid[(grid.rows - 1, grid.cols - 1)]
        );
    }
    println!(
        "mission={} field={}",
        loaded.get_global_property("mission", "?"),
        loaded.get_global_property("field", "?"),
    );

    std::fs::remove_file(&path).ok();
    Ok(())
}
