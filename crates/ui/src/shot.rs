//! A screenshot of the window saved to a file: PNG when the path ends in `.png` (the `png`
//! crate, already in the lock through egui's text rasterizer), a binary PPM otherwise. What
//! `--screenshot` and `ui.screenshot` write.

use std::io::{self, BufWriter, Write};
use std::path::Path;

use egui::ColorImage;

/// What a screenshot is saved as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Ppm,
}

impl Format {
    /// The format `path` names by its extension (`.png`, any case, or `.ppm`); none for another.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(Self::Png),
            "ppm" => Some(Self::Ppm),
            _ => None,
        }
    }
}

/// Saves `image` to `path`, as PNG when the path ends in `.png` and as a binary PPM otherwise.
///
/// # Errors
///
/// The file's I/O error, or the encoder's.
pub fn save(path: &Path, image: &ColorImage) -> io::Result<()> {
    let out = BufWriter::new(std::fs::File::create(path)?);
    match Format::of(path) {
        Some(Format::Png) => write_png(out, image),
        Some(Format::Ppm) | None => write_ppm(out, image),
    }
}

/// `image`'s pixels as RGB bytes, row by row: the window is opaque.
fn rgb(image: &ColorImage) -> Vec<u8> {
    image.pixels.iter().flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()]).collect()
}

/// `image` as an 8-bit RGB PNG.
///
/// # Errors
///
/// The writer's I/O error, or the encoder's (a size over `u32`, say).
pub fn write_png(out: impl Write, image: &ColorImage) -> io::Result<()> {
    let side = |n: usize| u32::try_from(n).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("an image {n} pixels wide")));
    let [width, height] = image.size;
    let mut encoder = png::Encoder::new(out, side(width)?, side(height)?);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    writer.write_image_data(&rgb(image)).map_err(io::Error::other)?;
    writer.finish().map_err(io::Error::other)
}

/// `image` as a binary PPM.
///
/// # Errors
///
/// The writer's I/O error.
pub fn write_ppm(mut out: impl Write, image: &ColorImage) -> io::Result<()> {
    write!(out, "P6\n{} {}\n255\n", image.size[0], image.size[1])?;
    out.write_all(&rgb(image))?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use egui::Color32;

    use super::*;

    fn image() -> ColorImage {
        let pixels = (0..6u8).map(|i| Color32::from_rgb(i * 40, 255 - i * 40, 7)).collect();
        ColorImage::new([3, 2], pixels)
    }

    #[test]
    fn a_png_reads_back_as_the_image() {
        let path = std::env::temp_dir().join(format!("worker24-shot-{}.PNG", std::process::id()));
        save(&path, &image()).unwrap();
        let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((frame.width, frame.height, frame.color_type, frame.bit_depth), (3, 2, png::ColorType::Rgb, png::BitDepth::Eight));
        assert_eq!(&pixels[..frame.buffer_size()], rgb(&image()).as_slice());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_ppm_is_its_header_and_its_bytes() {
        let mut out = Vec::new();
        write_ppm(&mut out, &image()).unwrap();
        let header = b"P6\n3 2\n255\n";
        assert_eq!(&out[..header.len()], header);
        assert_eq!(&out[header.len()..], rgb(&image()).as_slice());
        assert_eq!(
            (Format::of(Path::new("a.ppm")), Format::of(Path::new("a.Png")), Format::of(Path::new("a.jpg"))),
            (Some(Format::Ppm), Some(Format::Png), None)
        );
    }
}
