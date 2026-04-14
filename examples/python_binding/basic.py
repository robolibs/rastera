"""Minimal rastera Python binding demo.

Build with `make develop` in this directory first, which runs
`maturin develop --features python-extension`.
"""

import rastera

print("rastera version:", rastera.version())

r = rastera.Raster(datum=(52.0, 5.0, 10.0), resolution=1.0)
r.add_terrain_grid(8, 8)
r.add_occlusion_grid(8, 8)
r.set_global_property("mission", "python-demo")

for row in range(8):
    for col in range(8):
        r.grid_set(0, row, col, (row + col) * 16)

r.to_file("/tmp/rastera_python_basic.tif")
print("wrote /tmp/rastera_python_basic.tif")

loaded = rastera.read_file("/tmp/rastera_python_basic.tif")
print("grid_count:", loaded.grid_count())
print("grid_names:", loaded.grid_names())
print("shape of terrain:", loaded.grid_shape(loaded.find_grid("terrain")))
print("mission:", loaded.get_global_property("mission"))
print("terrain[3,4]:", loaded.grid_get(0, 3, 4))
