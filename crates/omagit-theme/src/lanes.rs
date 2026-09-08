//! Commit-graph lane colours.
//!
//! **Lanes never read the theme** (DESIGN-TOKENS §6). A lane's colour is
//! arbitrary and carries no meaning; it only has to be told apart from its
//! neighbour. No theme palette can guarantee eight separable hues — Matte Black
//! has zero — so they are generated instead, evenly spaced around the hue
//! circle.
//!
//! Only lightness and chroma vary per theme, so the lanes stay readable against
//! that theme's background. The hue spacing never varies.

use crate::color::Rgb;
use crate::oklch::Oklch;

/// Eight lanes before the palette repeats.
pub const LANE_COUNT: usize = 8;

/// Where the hue circle is entered, and the step between lanes.
const FIRST_HUE: f32 = 20.0;
const HUE_STEP: f32 = 360.0 / LANE_COUNT as f32;

/// Lanes other than the selected commit's drop to this opacity
/// (DESIGN-TOKENS §6). The selected node stays crisp: position, not hue,
/// carries the reading.
pub const UNSELECTED_LANE_OPACITY: f32 = 0.4;

/// `lane[i] = oklch(lightness, chroma, 20° + i × 45°)`, cyclic past eight.
pub fn lane_colors(count: usize, lightness: f32, chroma: f32) -> Vec<Rgb> {
    (0..count)
        .map(|index| lane_color(index, lightness, chroma))
        .collect()
}

/// The colour of one lane, by index. Cyclic: lane 8 is lane 0 again.
pub fn lane_color(index: usize, lightness: f32, chroma: f32) -> Rgb {
    let hue = FIRST_HUE + (index % LANE_COUNT) as f32 * HUE_STEP;
    Oklch::new(lightness, chroma, hue).to_rgb()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedded;
    use crate::oklch::contrast_ratio;

    /// Perceptual distance in Oklab, where Euclidean distance is the point of
    /// the space.
    fn perceptual_distance(a: Rgb, b: Rgb) -> f32 {
        let (a, b) = (Oklch::from(a), Oklch::from(b));
        let polar = |c: Oklch| {
            let radians = c.hue.to_radians();
            (
                c.lightness,
                c.chroma * radians.cos(),
                c.chroma * radians.sin(),
            )
        };
        let (al, aa, ab) = polar(a);
        let (bl, ba, bb) = polar(b);
        ((al - bl).powi(2) + (aa - ba).powi(2) + (ab - bb).powi(2)).sqrt()
    }

    /// Below this, two lanes side by side stop reading as different colours.
    /// Calibrated from the delivered themes, whose chroma runs 0.12–0.14: two
    /// hues 45° apart at chroma C sit 2·C·sin(22.5°) ≈ 0.765·C apart, so 0.12
    /// chroma gives ≈ 0.092 before any gamut mapping.
    const MIN_NEIGHBOUR_DISTANCE: f32 = 0.06;

    #[test]
    fn the_eight_hues_are_evenly_spaced() {
        let colors = lane_colors(LANE_COUNT, 0.74, 0.13);
        let hues: Vec<f32> = colors.iter().map(|c| Oklch::from(*c).hue).collect();
        for (index, hue) in hues.iter().enumerate() {
            let expected = FIRST_HUE + index as f32 * HUE_STEP;
            // Hue is circular, so 359° and 1° are two degrees apart.
            let gap = (hue - expected).rem_euclid(360.0);
            let drift = gap.min(360.0 - gap);
            assert!(
                drift < 12.0,
                "lane {index}: {hue}° vs {expected}° ({drift}° off)"
            );
        }
    }

    #[test]
    fn lanes_cycle_past_eight() {
        let many = lane_colors(LANE_COUNT + 3, 0.74, 0.13);
        assert_eq!(many[0], many[LANE_COUNT]);
        assert_eq!(many[2], many[LANE_COUNT + 2]);
    }

    #[test]
    fn neighbouring_lanes_stay_apart_on_every_theme() {
        // DESIGN-TOKENS §10 test 3: on a near-monochrome background (Matte
        // Black) and on a pastel one (the light themes) alike.
        for theme in embedded::catalogue() {
            let colors = lane_colors(
                LANE_COUNT,
                theme.tuning.lane_lightness,
                theme.tuning.lane_chroma,
            );
            for index in 0..LANE_COUNT {
                let next = (index + 1) % LANE_COUNT;
                let distance = perceptual_distance(colors[index], colors[next]);
                assert!(
                    distance >= MIN_NEIGHBOUR_DISTANCE,
                    "{}: lanes {index} and {next} are only {distance:.4} apart",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn every_lane_is_visible_against_its_background() {
        // A lane is a graphical object: WCAG 1.4.11 puts the floor at 3:1.
        // This is the half of the test that actually depends on the background,
        // and the reason lane lightness is a per-theme field.
        for theme in embedded::catalogue() {
            let bg = theme.tokens().bg;
            for (index, color) in lane_colors(
                LANE_COUNT,
                theme.tuning.lane_lightness,
                theme.tuning.lane_chroma,
            )
            .iter()
            .enumerate()
            {
                let ratio = contrast_ratio(*color, bg);
                assert!(
                    ratio >= 3.0,
                    "{}: lane {index} sits at {ratio:.2}:1 on its background",
                    theme.name
                );
            }
        }
    }
}
