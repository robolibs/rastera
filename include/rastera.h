#ifndef RASTERA_H
#define RASTERA_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RasteraRasterHandle RasteraRasterHandle;

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

typedef enum {
  RASTERA_GRID_KIND_UNKNOWN = 0,
  RASTERA_GRID_KIND_U8 = 1,
  RASTERA_GRID_KIND_I8 = 2,
  RASTERA_GRID_KIND_U16 = 3,
  RASTERA_GRID_KIND_I16 = 4,
  RASTERA_GRID_KIND_U32 = 5,
  RASTERA_GRID_KIND_I32 = 6,
  RASTERA_GRID_KIND_F32 = 7,
  RASTERA_GRID_KIND_F64 = 8,
  RASTERA_GRID_KIND_RGBA8 = 9,
} RasteraGridKind;

/* Error handling */
const char* rastera_last_error_message(void);
void rastera_string_free(char* ptr);

/* Raster lifecycle */
RasteraRasterHandle* rastera_raster_new(
    RasteraGeo3 datum,
    RasteraPose shift,
    double resolution);
RasteraRasterHandle* rastera_raster_from_file(const char* path);
bool rastera_raster_to_file(const RasteraRasterHandle* handle, const char* path);
void rastera_raster_free(RasteraRasterHandle* handle);

/* Raster metadata */
bool rastera_raster_get_datum(const RasteraRasterHandle* handle, RasteraGeo3* out);
bool rastera_raster_set_datum(RasteraRasterHandle* handle, RasteraGeo3 datum);
bool rastera_raster_get_shift(const RasteraRasterHandle* handle, RasteraPose* out);
bool rastera_raster_set_shift(RasteraRasterHandle* handle, RasteraPose shift);
double rastera_raster_get_resolution(const RasteraRasterHandle* handle);
bool rastera_raster_set_resolution(RasteraRasterHandle* handle, double resolution);
bool rastera_raster_set_global_property(
    RasteraRasterHandle* handle,
    const char* key,
    const char* value);
char* rastera_raster_get_global_property(
    const RasteraRasterHandle* handle,
    const char* key);

/* Grid layer management (indexed U8 grids) */
size_t rastera_raster_grid_count(const RasteraRasterHandle* handle);
bool rastera_raster_add_grid(
    RasteraRasterHandle* handle,
    size_t width,
    size_t height,
    const char* name,
    const char* grid_type);
bool rastera_raster_remove_grid(RasteraRasterHandle* handle, size_t index);
char* rastera_raster_grid_name(const RasteraRasterHandle* handle, size_t index);
char* rastera_raster_grid_type(const RasteraRasterHandle* handle, size_t index);
bool rastera_raster_grid_shape(
    const RasteraRasterHandle* handle,
    size_t index,
    size_t* out_rows,
    size_t* out_cols);
bool rastera_raster_grid_get_u8(
    const RasteraRasterHandle* handle,
    size_t index,
    size_t row,
    size_t col,
    uint8_t* out);
bool rastera_raster_grid_set_u8(
    RasteraRasterHandle* handle,
    size_t index,
    size_t row,
    size_t col,
    uint8_t value);
bool rastera_raster_grid_fill_u8(
    RasteraRasterHandle* handle,
    size_t index,
    uint8_t value);
bool rastera_raster_grid_find_by_name(
    const RasteraRasterHandle* handle,
    const char* name,
    size_t* out_index);

/* Standalone RGBA helpers */
bool rastera_write_rgba8(
    const char* path,
    size_t rows,
    size_t cols,
    const RasteraRgba* pixels,
    RasteraGeo3 datum,
    double resolution);
bool rastera_read_rgba8_size(const char* path, size_t* out_rows, size_t* out_cols);

#ifdef __cplusplus
}
#endif

#endif /* RASTERA_H */
