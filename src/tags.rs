use crate::error::{Error, Result};

pub const TIFF_RESERVED_MAX: u16 = 32_767;
pub const PRIVATE_TAG_MIN: u16 = 32_768;
pub const RASTERA_RESERVED_MIN: u16 = 50_000;
pub const RASTERA_RESERVED_MAX: u16 = 50_999;
pub const GLOBAL_PROPERTIES_BASE_TAG: u16 = 50_100;

pub fn is_valid_custom_tag(tag: u16) -> bool {
    tag >= PRIVATE_TAG_MIN
}

pub fn is_reserved_tiff_tag(tag: u16) -> bool {
    tag <= TIFF_RESERVED_MAX
}

pub fn validate_custom_tag(tag: u16) -> Result<()> {
    if is_valid_custom_tag(tag) {
        return Ok(());
    }

    Err(Error::Validation {
        field: "custom tag",
        expected: format!(">= {PRIVATE_TAG_MIN}"),
        actual: tag.to_string(),
    })
}

pub fn tag_name(tag: u16) -> &'static str {
    match tag {
        254 => "NewSubfileType",
        256 => "ImageWidth",
        257 => "ImageLength",
        258 => "BitsPerSample",
        259 => "Compression",
        262 => "PhotometricInterpretation",
        270 => "ImageDescription",
        273 => "StripOffsets",
        277 => "SamplesPerPixel",
        278 => "RowsPerStrip",
        279 => "StripByteCounts",
        282 => "XResolution",
        283 => "YResolution",
        284 => "PlanarConfiguration",
        296 => "ResolutionUnit",
        305 => "Software",
        306 => "DateTime",
        315 => "Artist",
        338 => "ExtraSamples",
        339 => "SampleFormat",
        33_550 => "ModelPixelScaleTag",
        33_922 => "ModelTiepointTag",
        34_264 => "ModelTransformationTag",
        34_735 => "GeoKeyDirectoryTag",
        34_736 => "GeoDoubleParamsTag",
        34_737 => "GeoAsciiParamsTag",
        42_113 => "GDAL_NODATA",
        _ => "Unknown",
    }
}

pub fn compression_name(compression: u16) -> String {
    match compression {
        1 => "None (uncompressed)".to_owned(),
        2 => "CCITT 1D".to_owned(),
        3 => "Group 3 Fax".to_owned(),
        4 => "Group 4 Fax".to_owned(),
        5 => "LZW".to_owned(),
        6 => "JPEG (old-style)".to_owned(),
        7 => "JPEG".to_owned(),
        8 => "Deflate (Adobe-style)".to_owned(),
        32_773 => "PackBits".to_owned(),
        32_946 => "Deflate (PKZIP-style)".to_owned(),
        _ => format!("Unknown ({compression})"),
    }
}

#[cfg(test)]
mod tests {
    use super::{PRIVATE_TAG_MIN, compression_name, is_reserved_tiff_tag, validate_custom_tag};

    #[test]
    fn custom_tag_validation_matches_tiff_private_range() {
        assert!(validate_custom_tag(PRIVATE_TAG_MIN).is_ok());
        assert!(validate_custom_tag(PRIVATE_TAG_MIN - 1).is_err());
        assert!(is_reserved_tiff_tag(300));
        assert!(!is_reserved_tiff_tag(PRIVATE_TAG_MIN));
    }

    #[test]
    fn compression_names_cover_known_values() {
        assert_eq!(compression_name(1), "None (uncompressed)");
        assert_eq!(compression_name(7), "JPEG");
        assert_eq!(compression_name(999), "Unknown (999)");
    }
}
