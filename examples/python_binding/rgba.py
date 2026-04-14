"""Write and read an RGBA GeoTIFF using the Python bindings."""

import rastera

pixels = [
    (255, 0, 0, 255),
    (0, 255, 0, 255),
    (0, 0, 255, 255),
    (255, 255, 255, 255),
]
rastera.write_rgba8(
    "/tmp/rastera_python_rgba.tif",
    rows=2,
    cols=2,
    pixels=pixels,
    datum=(52.0, 5.0, 0.0),
    resolution=0.5,
)
print("wrote /tmp/rastera_python_rgba.tif")

payload = rastera.read_rgba8("/tmp/rastera_python_rgba.tif")
print("rows:", payload["rows"], "cols:", payload["cols"])
for idx, px in enumerate(payload["pixels"]):
    print(f"  pixel {idx}: r={px[0]} g={px[1]} b={px[2]} a={px[3]}")
