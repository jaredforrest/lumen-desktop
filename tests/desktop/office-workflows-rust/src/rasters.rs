//! Exact painted-pixel differences for independently rasterized PDF pages.
use crate::{Result, ensure};
use serde_json::{Value, json};
use std::fs::File;
use std::path::PathBuf;

struct Image {
    width: usize,
    height: usize,
    pixels: Vec<[u8; 3]>,
}
fn read(path: &PathBuf) -> Result<Image> {
    let file = File::open(path)?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer)?;
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        _ => return Err("Unsupported PNG color".into()),
    };
    let mut pixels = Vec::with_capacity(info.width as usize * info.height as usize);
    for chunk in buffer[..info.buffer_size()].chunks_exact(channels) {
        pixels.push(if channels <= 2 {
            [chunk[0]; 3]
        } else {
            [chunk[0], chunk[1], chunk[2]]
        });
    }
    Ok(Image {
        width: info.width as usize,
        height: info.height as usize,
        pixels,
    })
}
pub fn nonblack_pixels(path: &PathBuf) -> Result<(usize, usize, usize)> {
    let image = read(path)?;
    let count = image
        .pixels
        .iter()
        .filter(|pixel| **pixel != [0, 0, 0])
        .count();
    Ok((image.width, image.height, count))
}
pub fn compare_rasters(before: &[PathBuf], after: &[PathBuf]) -> Result<Vec<Value>> {
    let mut changes = Vec::new();
    for index in 0..before.len().max(after.len()) {
        if index >= before.len() || index >= after.len() {
            changes.push(json!({"page":index+1,"kind":"missing_page"}));
            continue;
        }
        let left = read(&before[index])?;
        let right = read(&after[index])?;
        if left.width != right.width || left.height != right.height {
            changes.push(json!({"page":index+1,"kind":"page_dimensions","before":[left.width,left.height],"after":[right.width,right.height]}));
            continue;
        }
        ensure(
            left.pixels.len() == right.pixels.len(),
            "PNG pixel count mismatch",
        )?;
        let mut min_x = left.width;
        let mut min_y = left.height;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        let mut count = 0usize;
        for (index, (a, b)) in left.pixels.iter().zip(&right.pixels).enumerate() {
            if a != b {
                let x = index % left.width;
                let y = index / left.width;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
                count += 1;
            }
        }
        if count > 0 {
            changes.push(json!({"page":index+1,"kind":"painted_pixels","bounds":[min_x,min_y,max_x,max_y],"changedPixels":count}));
        }
    }
    Ok(changes)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    fn write(path: &PathBuf, pixels: &[u8]) {
        let file = File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    #[test]
    fn detects_single_pixel_and_missing_page() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a.png");
        let b = root.path().join("b.png");
        let mut pixels = vec![255; 12];
        write(&a, &pixels);
        pixels[9] = 254;
        write(&b, &pixels);
        assert_eq!(
            json!(compare_rasters(&[a.clone()], &[a.clone()]).unwrap()),
            json!([])
        );
        assert_eq!(
            json!(compare_rasters(&[a.clone()], &[b]).unwrap()),
            json!([{"page":1,"kind":"painted_pixels","bounds":[1,1,2,2],"changedPixels":1}])
        );
        assert_eq!(
            json!(compare_rasters(&[a], &[]).unwrap()),
            json!([{"page":1,"kind":"missing_page"}])
        );
    }
}
