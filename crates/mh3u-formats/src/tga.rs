//! TGA images (the dump's `meta/iconTex.tga`, the game's icon), as RGBA. Only what Wii U meta images use, with room to spare:
//! true-colour (type 2) or run-length true-colour (type 10), 24 or 32 bits a pixel, either row order. Bounded: a damaged file
//! gives an error, never a panic or an oversized allocation.

use anyhow::{Result, bail, ensure};

/// The largest side accepted.
pub const MOST_SIDE: u16 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// RGBA, rows from the top.
    pub rgba: Vec<u8>,
}

pub fn decode(data: &[u8]) -> Result<Image> {
    ensure!(data.len() >= 18, "TGA: too short for a header");
    let id_len = usize::from(data[0]);
    let (color_map, kind) = (data[1], data[2]);
    let le = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
    let (width, height, bpp, descriptor) = (le(12), le(14), data[16], data[17]);
    ensure!(color_map == 0, "TGA: colour-mapped images are not read");
    ensure!(kind == 2 || kind == 10, "TGA: image type {kind} is not read (only true colour)");
    ensure!(bpp == 24 || bpp == 32, "TGA: {bpp} bits a pixel is not read");
    ensure!(
        width > 0 && height > 0 && width <= MOST_SIDE && height <= MOST_SIDE,
        "TGA: size {width}x{height}"
    );
    let px = usize::from(bpp / 8);
    let n = usize::from(width) * usize::from(height);
    let mut src = data.get(18 + id_len..).unwrap_or(&[]);
    let mut pixels: Vec<u8> = Vec::with_capacity(n * 4);
    let mut push = |p: &[u8]| {
        pixels.extend_from_slice(&[p[2], p[1], p[0], if px == 4 { p[3] } else { 255 }]);
    };
    if kind == 2 {
        ensure!(src.len() >= n * px, "TGA: the pixels are cut short");
        src[..n * px].chunks_exact(px).for_each(&mut push);
    } else {
        let mut done = 0;
        while done < n {
            let Some((&head, rest)) = src.split_first() else {
                bail!("TGA: the run-length data is cut short")
            };
            let count = usize::from(head & 0x7f) + 1;
            ensure!(done + count <= n, "TGA: a run goes past the image");
            if head & 0x80 != 0 {
                ensure!(rest.len() >= px, "TGA: the run-length data is cut short");
                (0..count).for_each(|_| push(&rest[..px]));
                src = &rest[px..];
            } else {
                ensure!(rest.len() >= count * px, "TGA: the run-length data is cut short");
                rest[..count * px].chunks_exact(px).for_each(&mut push);
                src = &rest[count * px..];
            }
            done += count;
        }
    }
    // bit 5 of the descriptor: rows from the top; otherwise from the bottom
    if descriptor & 0x20 == 0 {
        let row = usize::from(width) * 4;
        let flipped: Vec<u8> = pixels.chunks_exact(row).rev().flatten().copied().collect();
        pixels = flipped;
    }
    Ok(Image {
        width: u32::from(width),
        height: u32::from(height),
        rgba: pixels,
    })
}

impl Image {
    /// The image scaled to `side` x `side` (an average of the pixels each covers when smaller, the nearest when larger).
    pub fn resized(&self, side: u32) -> Image {
        let side = side.max(1);
        let mut rgba = Vec::with_capacity((side * side * 4) as usize);
        for y in 0..side {
            let (y0, y1) = span(y, side, self.height);
            for x in 0..side {
                let (x0, x1) = span(x, side, self.width);
                let mut sum = [0u32; 4];
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let i = ((sy * self.width + sx) * 4) as usize;
                        for (s, v) in sum.iter_mut().zip(&self.rgba[i..i + 4]) {
                            *s += u32::from(*v);
                        }
                    }
                }
                let count = (y1 - y0) * (x1 - x0);
                rgba.extend(sum.iter().map(|s| (s / count) as u8));
            }
        }
        Image {
            width: side,
            height: side,
            rgba,
        }
    }
}

/// The source pixels [from, to) that output pixel `i` of `out` covers, out of `len` (at least one).
fn span(i: u32, out: u32, len: u32) -> (u32, u32) {
    let from = (u64::from(i) * u64::from(len) / u64::from(out)) as u32;
    let to = ((u64::from(i + 1) * u64::from(len)).div_ceil(u64::from(out)) as u32).max(from + 1);
    (from.min(len - 1), to.min(len))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A TGA made here (no game file): 2x2, 32 bits, rows from the bottom.
    fn tga(kind: u8, descriptor: u8, body: &[u8]) -> Vec<u8> {
        let mut h = vec![0, 0, kind, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 32, descriptor];
        h.extend_from_slice(body);
        h
    }

    // BGRA: blue, green / red, white (bottom row first)
    const BOTTOM_UP: [u8; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];

    #[test]
    fn plain_and_run_length_images_read_alike() {
        let plain = decode(&tga(2, 8, &BOTTOM_UP)).unwrap();
        assert_eq!((plain.width, plain.height), (2, 2));
        // top row first: red, white, then blue, green
        assert_eq!(&plain.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&plain.rgba[8..12], &[0, 0, 255, 255]);
        // two raw packets of 2 pixels each
        let mut rle = vec![0x01];
        rle.extend_from_slice(&BOTTOM_UP[..8]);
        rle.push(0x01);
        rle.extend_from_slice(&BOTTOM_UP[8..]);
        assert_eq!(decode(&tga(10, 8, &rle)).unwrap(), plain);
        let run = decode(&tga(10, 8, &[0x83, 1, 2, 3, 4])).unwrap();
        assert_eq!(run.rgba, [3, 2, 1, 4].repeat(4), "a run repeats one pixel");
        let top = decode(&tga(2, 0x28, &BOTTOM_UP)).unwrap();
        assert_eq!(&top.rgba[..4], &[0, 0, 255, 255], "rows from the top are kept");
    }

    #[test]
    fn damaged_files_are_errors() {
        assert!(decode(&[0; 10]).is_err());
        assert!(decode(&tga(2, 8, &BOTTOM_UP[..8])).is_err());
        assert!(decode(&tga(10, 8, &[0xff, 1, 2, 3, 4])).is_err(), "a run past the image");
        assert!(decode(&tga(1, 8, &BOTTOM_UP)).is_err());
        let mut huge = tga(2, 8, &[]);
        huge[12..16].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
        assert!(decode(&huge).is_err());
    }

    #[test]
    fn resizing_averages_down_and_repeats_up() {
        let img = decode(&tga(2, 8, &BOTTOM_UP)).unwrap();
        let one = img.resized(1);
        assert_eq!(one.rgba, vec![127, 127, 127, 255]);
        let four = img.resized(4);
        assert_eq!(four.rgba.len(), 64);
        assert_eq!(&four.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&four.rgba[60..], &[0, 255, 0, 255]);
    }
}
