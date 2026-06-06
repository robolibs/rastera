#ifndef RASTERA_H
#define RASTERA_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define TIFF_RESERVED_MAX 32767

#define PRIVATE_TAG_MIN 32768

#define RASTERA_RESERVED_MIN 50000

#define RASTERA_RESERVED_MAX 50999

#define GLOBAL_PROPERTIES_BASE_TAG 50100

typedef struct RasteraRaster RasteraRaster;

typedef struct {
  double latitude;
  double longitude;
  double altitude;
} RasteraGeo3;

typedef struct {
  double x;
  double y;
  double z;
} RasteraPoint3;

typedef struct {
  double w;
  double x;
  double y;
  double z;
} RasteraQuat;

typedef struct {
  RasteraPoint3 point;
  RasteraQuat rotation;
} RasteraPose;

typedef struct {
  uint8_t r;
  uint8_t g;
  uint8_t b;
  uint8_t a;
} RasteraRgba;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

const char *rastera_last_error_message(void);

void rastera_string_free(char *ptr);

RasteraRaster *rastera_raster_new(RasteraGeo3 datum, RasteraPose shift, double resolution);

RasteraRaster *rastera_raster_from_file(const char *path);

bool rastera_raster_to_file(const RasteraRaster *handle, const char *path);

void rastera_raster_free(RasteraRaster *handle);

bool rastera_raster_get_datum(const RasteraRaster *handle, RasteraGeo3 *out);

bool rastera_raster_set_datum(RasteraRaster *handle, RasteraGeo3 datum);

bool rastera_raster_get_shift(const RasteraRaster *handle, RasteraPose *out);

bool rastera_raster_set_shift(RasteraRaster *handle, RasteraPose shift);

double rastera_raster_get_resolution(const RasteraRaster *handle);

bool rastera_raster_set_resolution(RasteraRaster *handle, double resolution);

bool rastera_raster_set_global_property(RasteraRaster *handle, const char *key, const char *value);

char *rastera_raster_get_global_property(const RasteraRaster *handle, const char *key);

uintptr_t rastera_raster_grid_count(const RasteraRaster *handle);

bool rastera_raster_add_grid(RasteraRaster *handle,
                             uintptr_t width,
                             uintptr_t height,
                             const char *name,
                             const char *grid_type);

bool rastera_raster_remove_grid(RasteraRaster *handle, uintptr_t index);

char *rastera_raster_grid_name(const RasteraRaster *handle, uintptr_t index);

char *rastera_raster_grid_type(const RasteraRaster *handle, uintptr_t index);

bool rastera_raster_grid_shape(const RasteraRaster *handle,
                               uintptr_t index,
                               uintptr_t *out_rows,
                               uintptr_t *out_cols);

bool rastera_raster_grid_get_u8(const RasteraRaster *handle,
                                uintptr_t index,
                                uintptr_t row,
                                uintptr_t col,
                                uint8_t *out);

bool rastera_raster_grid_set_u8(RasteraRaster *handle,
                                uintptr_t index,
                                uintptr_t row,
                                uintptr_t col,
                                uint8_t value);

bool rastera_raster_grid_fill_u8(RasteraRaster *handle, uintptr_t index, uint8_t value);

bool rastera_raster_grid_find_by_name(const RasteraRaster *handle,
                                      const char *name,
                                      uintptr_t *out_index);

bool rastera_write_rgba8(const char *path,
                         uintptr_t rows,
                         uintptr_t cols,
                         const RasteraRgba *pixels,
                         RasteraGeo3 datum,
                         double resolution);

bool rastera_read_rgba8_size(const char *path, uintptr_t *out_rows, uintptr_t *out_cols);

/**
 * Read an RGBA raster into a caller-provided buffer. `capacity` must be at
 * least `rows * cols`, as reported by `rastera_read_rgba8_size`.
 */
bool rastera_read_rgba8_into(const char *path, RasteraRgba *out_pixels, uintptr_t capacity);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* RASTERA_H */
