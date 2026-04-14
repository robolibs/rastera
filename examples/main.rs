//! Minimal rastera smoke-test example.
//!
//! Creates a 16x16 u8 raster with a diagonal gradient, writes it to a
//! temporary GeoTIFF, reads it back, and prints a few sanity-check values.
//!
//! Run with: `cargo run --example main`

use std::env;

use datapod::{Geo, Point, Pose, Quaternion};
use rastera::Raster;

fn main() -> rastera::Result<()> {
    let mut raster = Raster::new(
        Geo::new(52.0, 5.0, 10.0),
        Pose {
            point: Point::new(0.0, 0.0, 0.0),
            rotation: Quaternion::new(1.0, 0.0, 0.0, 0.0),
        },
        1.0,
    );
    raster.add_terrain_grid(16, 16, "terrain");
    raster.set_global_property("mission", "smoke");

    {
        let grid = &mut raster.get_grid_mut(0).unwrap().grid;
        for r in 0..grid.rows {
            for c in 0..grid.cols {
                grid[(r, c)] = ((r + c) * 8) as u8;
            }
        }
    }

    let path = env::temp_dir().join("rastera_main_example.tif");
    raster.to_file(&path)?;
    println!("wrote {}", path.display());

    let loaded = Raster::from_file(&path)?;
    let grid = &loaded.get_grid_by_name("terrain").unwrap().grid;
    println!("grid_count = {}", loaded.grid_count());
    println!("shape      = {} x {}", grid.rows, grid.cols);
    println!(
        "diagonal   = [{}, {}, {}, {}, ...]",
        grid[(0, 0)],
        grid[(1, 1)],
        grid[(2, 2)],
        grid[(3, 3)]
    );
    println!(
        "mission    = {}",
        loaded.get_global_property("mission", "unset")
    );

    std::fs::remove_file(&path).ok();
    Ok(())
}
