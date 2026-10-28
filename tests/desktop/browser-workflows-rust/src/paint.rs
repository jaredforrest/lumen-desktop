use crate::{Result, fail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs::File;
use std::io::Cursor;
use std::path::Path;

#[derive(Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 3]>,
}
impl Image {
    pub fn solid(width: usize, height: usize, color: [u8; 3]) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; width * height],
        }
    }
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        self.pixels[y * self.width + x]
    }
    pub fn set(&mut self, x: usize, y: usize, color: [u8; 3]) {
        self.pixels[y * self.width + x] = color;
    }
    pub fn from_png(bytes: &[u8]) -> Result<Self> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer)?;
        let count = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            _ => return Err(fail("Unsupported browser screenshot format")),
        };
        let mut pixels = Vec::with_capacity((info.width * info.height) as usize);
        for chunk in buffer[..info.buffer_size()].chunks_exact(count) {
            pixels.push([chunk[0], chunk[1], chunk[2]]);
        }
        Ok(Self {
            width: info.width as usize,
            height: info.height as usize,
            pixels,
        })
    }
    pub fn load_png(path: &Path) -> Result<Self> {
        Self::from_png(&std::fs::read(path)?)
    }
    pub fn save_png(&self, path: &Path) -> Result<()> {
        let mut encoder =
            png::Encoder::new(File::create(path)?, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        let bytes = self
            .pixels
            .iter()
            .flat_map(|pixel| pixel.iter().copied())
            .collect::<Vec<_>>();
        writer.write_image_data(&bytes)?;
        Ok(())
    }
    pub fn crop(&self, left: usize, top: usize, width: usize, height: usize) -> Result<Self> {
        if left + width > self.width || top + height > self.height {
            return Err(fail("Reference crop escaped screenshot"));
        }
        let mut pixels = Vec::with_capacity(width * height);
        for y in top..top + height {
            pixels.extend_from_slice(
                &self.pixels[y * self.width + left..y * self.width + left + width],
            );
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}
fn delta(a: [u8; 3], b: [u8; 3]) -> u8 {
    (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap()
}
pub fn match_paint(reference: &Image, screen: &Image, origin: (f64, f64)) -> Value {
    let mut counts = HashMap::new();
    for pixel in &reference.pixels {
        *counts.entry(*pixel).or_insert(0_usize) += 1;
    }
    let background = counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(pixel, _)| pixel)
        .unwrap_or([0; 3]);
    let signature: Vec<_> = reference
        .pixels
        .iter()
        .enumerate()
        .filter(|(_, pixel)| delta(**pixel, background) > 12)
        .map(|(i, _)| i)
        .collect();
    let base = json!({"painted":false,"signaturePixels":signature.len(),"maximumRasterOffset":1,"colorTolerance":3});
    if signature.len() < 16 {
        return base;
    }
    let (x, y) = (origin.0.round() as isize, origin.1.round() as isize);
    let mut best = None;
    let mut offsets = vec![(0, 0)];
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx != 0 || dy != 0 {
                offsets.push((dx, dy));
            }
        }
    }
    for (dx, dy) in offsets {
        let (left, top) = (x + dx, y + dy);
        if left < 0
            || top < 0
            || left as usize + reference.width > screen.width
            || top as usize + reference.height > screen.height
        {
            continue;
        }
        let mut mismatch = 0;
        let mut signature_mismatch = 0;
        let mut differences = Vec::with_capacity(reference.pixels.len());
        for py in 0..reference.height {
            for px in 0..reference.width {
                let i = py * reference.width + px;
                let difference = delta(
                    reference.pixels[i],
                    screen.pixel(left as usize + px, top as usize + py),
                );
                differences.push(difference);
                if difference > 3 {
                    mismatch += 1;
                }
            }
        }
        for i in &signature {
            if differences[*i] > 3 {
                signature_mismatch += 1;
            }
        }
        let mf = mismatch as f64 / reference.pixels.len() as f64;
        let sf = signature_mismatch as f64 / signature.len() as f64;
        let mut value = json!({"painted":false,"signaturePixels":signature.len(),"maximumRasterOffset":1,"colorTolerance":3,"mismatchFraction":mf,"signatureMismatchFraction":sf,"rasterOffset":[dx,dy],"origin":[left,top],"size":[reference.width,reference.height]});
        if best
            .as_ref()
            .is_none_or(|(_, old_mf, old_sf): &(Value, f64, f64)| (sf, mf) < (*old_sf, *old_mf))
        {
            best = Some((value.clone(), mf, sf));
        }
        if mf <= 0.002 && sf <= 0.01 {
            value["painted"] = json!(true);
            return value;
        }
    }
    best.map(|v| v.0).unwrap_or(base)
}
