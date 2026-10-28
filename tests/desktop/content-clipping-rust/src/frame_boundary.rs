//! Geometry oracle for declared frame boundaries in native pixel fixtures.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryError {
    NonFinite,
    InvalidDimensions,
    InvalidOutputMapping,
}
impl std::fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid frame geometry: {self:?}")
    }
}
impl std::error::Error for GeometryError {}

/// Test a logical pixel centre against the frame and physical raster tolerance.
pub fn outside_frame(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
    tolerance: f64,
) -> Result<bool, GeometryError> {
    if ![x, y, width, height, radius, tolerance]
        .into_iter()
        .all(f64::is_finite)
    {
        return Err(GeometryError::NonFinite);
    }
    if width <= 0.0 || height <= 0.0 || radius < 0.0 || tolerance < 0.0 {
        return Err(GeometryError::InvalidDimensions);
    }
    if x < -tolerance || y < -tolerance || x > width + tolerance || y > height + tolerance {
        return Ok(true);
    }
    let radius = radius.min(width / 2.0).min(height / 2.0);
    if radius == 0.0 {
        return Ok(false);
    }
    let cx = x.clamp(radius, width - radius);
    let cy = y.clamp(radius, height - radius);
    Ok((x - cx).powi(2) + (y - cy).powi(2) > (radius + tolerance).powi(2))
}

/// Match the scene's absolute output-local scale rounding, with ties away from zero.
pub fn raster_origin(value: f64, scale: f64) -> Result<i64, GeometryError> {
    if !value.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return Err(GeometryError::InvalidOutputMapping);
    }
    let physical = value * scale;
    if !physical.is_finite() || physical < i64::MIN as f64 || physical > i64::MAX as f64 {
        return Err(GeometryError::InvalidOutputMapping);
    }
    Ok(if physical >= 0.0 {
        (physical + 0.5).floor() as i64
    } else {
        (physical - 0.5).ceil() as i64
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_client_keeps_corners() {
        for (x, y) in [(0., 0.), (200., 0.), (0., 100.), (200., 100.), (100., 50.)] {
            assert_eq!(outside_frame(x, y, 200., 100., 0., 0.), Ok(false));
        }
        assert_eq!(outside_frame(-0.01, 50., 200., 100., 0., 0.), Ok(true));
    }

    #[test]
    fn all_rounded_corners_reject_square_child_pixels() {
        for (x, y) in [(0., 0.), (200., 0.), (0., 100.), (200., 100.)] {
            assert_eq!(outside_frame(x, y, 200., 100., 12., 0.), Ok(true));
        }
        for (x, y) in [(12., 0.), (188., 0.), (0., 12.), (200., 12.), (100., 50.)] {
            assert_eq!(outside_frame(x, y, 200., 100., 12., 0.), Ok(false));
        }
    }

    #[test]
    fn physical_pixel_tolerance_scales() {
        for scale in [1., 1.25, 1.5, 2.] {
            assert_eq!(
                outside_frame(-0.9 / scale, 50., 200., 100., 0., 1. / scale),
                Ok(false)
            );
            assert_eq!(
                outside_frame(-1.1 / scale, 50., 200., 100., 0., 1. / scale),
                Ok(true)
            );
            assert_eq!(
                outside_frame(12., -0.9 / scale, 200., 100., 12., 1. / scale),
                Ok(false)
            );
            assert_eq!(
                outside_frame(12., -1.1 / scale, 200., 100., 12., 1. / scale),
                Ok(true)
            );
        }
    }

    #[test]
    fn output_origin_uses_native_rounding() {
        assert_eq!(raster_origin(245., 1.5), Ok(368));
        assert_eq!(raster_origin(243., 1.5), Ok(365));
        assert_eq!(raster_origin(-243., 1.5), Ok(-365));
        for scale in [1., 1.25, 1.5, 2.] {
            for value in [0., 1., 243., 245., -243.] {
                assert!((raster_origin(value, scale).unwrap() as f64 - value * scale).abs() <= 0.5);
            }
        }
        assert_eq!(
            raster_origin(1., 0.),
            Err(GeometryError::InvalidOutputMapping)
        );
    }

    #[test]
    fn invalid_geometry_fails_closed() {
        for geometry in [
            [0., 0., 0., 100., 12., 1.],
            [0., 0., 200., 100., -1., 1.],
            [0., 0., 200., 100., 12., -1.],
            [f64::NAN, 0., 200., 100., 12., 1.],
        ] {
            assert!(
                outside_frame(
                    geometry[0],
                    geometry[1],
                    geometry[2],
                    geometry[3],
                    geometry[4],
                    geometry[5]
                )
                .is_err()
            );
        }
    }
}
