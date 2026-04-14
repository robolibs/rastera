#include <stdio.h>
#include <string.h>

#include "../../include/rastera.h"

static const char* OUT_PATH = "/tmp/rastera_c_demo.tif";

int main(void) {
  RasteraGeo3 datum = {52.0, 5.0, 10.0};
  RasteraPose shift = {
      .point = {0.0, 0.0, 0.0},
      .rotation = {1.0, 0.0, 0.0, 0.0},
  };

  RasteraRasterHandle* raster = rastera_raster_new(datum, shift, 1.0);
  if (raster == NULL) {
    fprintf(stderr, "rastera_raster_new failed: %s\n",
            rastera_last_error_message());
    return 1;
  }

  if (!rastera_raster_add_grid(raster, 8, 8, "terrain", "terrain")) {
    fprintf(stderr, "add_grid failed: %s\n", rastera_last_error_message());
    rastera_raster_free(raster);
    return 1;
  }
  if (!rastera_raster_add_grid(raster, 8, 8, "occlusion", "occlusion")) {
    fprintf(stderr, "add_grid failed: %s\n", rastera_last_error_message());
    rastera_raster_free(raster);
    return 1;
  }

  if (!rastera_raster_grid_fill_u8(raster, 0, 10)) {
    fprintf(stderr, "fill failed: %s\n", rastera_last_error_message());
    rastera_raster_free(raster);
    return 1;
  }
  if (!rastera_raster_grid_set_u8(raster, 0, 3, 4, 250)) {
    fprintf(stderr, "set failed: %s\n", rastera_last_error_message());
    rastera_raster_free(raster);
    return 1;
  }

  if (!rastera_raster_set_global_property(raster, "mission", "c-abi-demo")) {
    fprintf(stderr, "set_global_property failed: %s\n",
            rastera_last_error_message());
    rastera_raster_free(raster);
    return 1;
  }

  if (!rastera_raster_to_file(raster, OUT_PATH)) {
    fprintf(stderr, "to_file failed: %s\n", rastera_last_error_message());
    rastera_raster_free(raster);
    return 1;
  }
  printf("wrote %s\n", OUT_PATH);
  rastera_raster_free(raster);

  RasteraRasterHandle* loaded = rastera_raster_from_file(OUT_PATH);
  if (loaded == NULL) {
    fprintf(stderr, "from_file failed: %s\n", rastera_last_error_message());
    return 1;
  }
  printf("grid_count = %zu\n", rastera_raster_grid_count(loaded));

  size_t index = 0;
  if (!rastera_raster_grid_find_by_name(loaded, "terrain", &index)) {
    fprintf(stderr, "find failed: %s\n", rastera_last_error_message());
    rastera_raster_free(loaded);
    return 1;
  }
  size_t rows = 0, cols = 0;
  if (!rastera_raster_grid_shape(loaded, index, &rows, &cols)) {
    fprintf(stderr, "shape failed: %s\n", rastera_last_error_message());
    rastera_raster_free(loaded);
    return 1;
  }
  printf("terrain shape = %zu x %zu\n", rows, cols);

  uint8_t marker = 0;
  if (!rastera_raster_grid_get_u8(loaded, index, 3, 4, &marker)) {
    fprintf(stderr, "get failed: %s\n", rastera_last_error_message());
    rastera_raster_free(loaded);
    return 1;
  }
  printf("terrain[3][4] = %u (expected 250)\n", marker);

  char* name = rastera_raster_grid_name(loaded, 0);
  if (name != NULL) {
    printf("first grid name = %s\n", name);
    rastera_string_free(name);
  }

  char* mission = rastera_raster_get_global_property(loaded, "mission");
  if (mission != NULL) {
    printf("mission = %s\n", mission);
    rastera_string_free(mission);
  }

  rastera_raster_free(loaded);

  RasteraRgba pixels[4] = {
      {255, 0, 0, 255},
      {0, 255, 0, 255},
      {0, 0, 255, 255},
      {255, 255, 255, 255},
  };
  const char* rgba_path = "/tmp/rastera_c_demo_rgba.tif";
  if (!rastera_write_rgba8(rgba_path, 2, 2, pixels, datum, 0.5)) {
    fprintf(stderr, "write_rgba8 failed: %s\n",
            rastera_last_error_message());
    return 1;
  }
  size_t rgba_rows = 0, rgba_cols = 0;
  if (!rastera_read_rgba8_size(rgba_path, &rgba_rows, &rgba_cols)) {
    fprintf(stderr, "read_rgba8_size failed: %s\n",
            rastera_last_error_message());
    return 1;
  }
  printf("rgba %s -> %zu x %zu\n", rgba_path, rgba_rows, rgba_cols);

  return 0;
}
