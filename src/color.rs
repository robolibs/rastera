use rgb::RGBA8;

pub type Rgba8 = RGBA8;

pub fn rgba_to_luma_u8(color: Rgba8) -> u8 {
    let luma = 0.299 * f64::from(color.r) + 0.587 * f64::from(color.g) + 0.114 * f64::from(color.b);
    luma.clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::{Rgba8, rgba_to_luma_u8};

    #[test]
    fn luma_conversion_matches_expected_range() {
        let color = Rgba8::new(255, 128, 64, 255);
        let luma = rgba_to_luma_u8(color);

        assert!((0..=255).contains(&luma));
        assert!(luma > 0);
    }
}
