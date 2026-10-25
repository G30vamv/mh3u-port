//! The graphics API (`gx2`). For now only what the game needs to size its buffers: nothing is drawn yet.
//!
//! The surface sizes follow the console's tiling rules (see `mh3u_formats::tiling`), as `GX2CalcSurfaceSizeAndAlignment` does.
//!
//! Tiling apertures (`GX2AllocateTilingApertureEx`) give the CPU a linear view of a tiled surface. The game reads one to find the
//! colour under the cursor in character creation's custom colour picker (issue #31: without them every custom colour was black).

use crate::cafe::SimpleHandler;
use crate::guest::Guest;
use mh3u_formats::tiling::{self, Surface, TileMode};
use std::collections::BTreeMap;

pub fn find(name: &str) -> Option<SimpleHandler> {
    Some(match name {
        "gx2:GX2CalcSurfaceSizeAndAlignment" => |_, g, c| calc_surface(g, c.r[3]),
        // the GPU generation the shader files are checked against (Latte is 2)
        "gx2:GX2TempGetGPUVersion" => |_, _, c| c.r[3] = 2,
        "gx2:GX2CalcTVSize" => |_, g, c| {
            // (render mode, format, buffering mode, *size, *scale needed)
            let (w, h) = match c.r[3] {
                1 | 2 => (854, 480),
                3 => (1280, 720),
                4 => (1280, 720),
                _ => (1920, 1080),
            };
            write_size(g, c.r[6], c.r[7], w, h, c.r[4], c.r[5]);
        },
        "gx2:GX2CalcDRCSize" => |_, g, c| write_size(g, c.r[6], c.r[7], 854, 480, c.r[4], c.r[5]),
        // (format): the bits in one element (a texel, or a 4 x 4 block of a compressed format)
        "gx2:GX2GetSurfaceFormatBits" => |_, _, c| c.r[3] = element(c.r[3]).0 * 8,
        // (surface, level): the level's pitch in elements
        "gx2:GX2GetSurfaceMipPitch" => |_, g, c| c.r[3] = mip_pitch(g, c.r[3], c.r[4]),
        // (surface, level, slice, endian swap mode, *handle, *address)
        "gx2:GX2AllocateTilingApertureEx" => |os, g, c| {
            let (handle, address) = os.apertures.open(g, c.r[3], c.r[4], c.r[5], c.r[6]).unwrap_or((0, 0));
            if c.r[7] != 0 {
                g.put_u32(c.r[7], handle);
            }
            if c.r[8] != 0 {
                g.put_u32(c.r[8], address);
            }
        },
        // (handle): what was written through the aperture goes back into the surface
        "gx2:GX2FreeTilingAperture" => |os, g, c| os.apertures.close(g, c.r[3]),
        _ => return None,
    })
}

/// Bytes per element (per 4x4 block for the compressed formats) and whether the format is block compressed.
fn element(format: u32) -> (u32, bool) {
    match format & 0x3f {
        0x01..=0x04 => (1, false),
        0x05..=0x0c => (2, false),
        0x0d..=0x1a => (4, false),
        0x1b..=0x1e => (8, false),
        0x1f..=0x23 => (16, false),
        0x31 | 0x34 => (8, true),
        0x32 | 0x33 | 0x35 => (16, true),
        _ => (4, false),
    }
}

fn align(v: u32, to: u32) -> u32 {
    v.div_ceil(to) * to
}

/// The size of a display buffer set: the bytes the game must allocate, and whether the picture needs scaling (it does not here).
fn write_size(g: &Guest, size: u32, scale: u32, w: u32, h: u32, format: u32, buffering: u32) {
    let (bpp, _) = element(format);
    let one = align(align(w, 64) * align(h, 8) * bpp, 0x1000);
    if size != 0 {
        g.put_u32(size, one * buffering.max(1));
    }
    if scale != 0 {
        g.put_u32(scale, 0);
    }
}

/// The number of mip levels a surface can have (its largest side's bits).
fn level_count(dim: u32, w: u32, h: u32, d: u32, asked: u32) -> u32 {
    if asked <= 1 {
        return 1;
    }
    let bits = |v: u32| 32 - v.leading_zeros();
    let mut levels = bits(w).max(bits(h));
    if dim == 2 {
        levels = levels.max(bits(d));
    }
    levels
}

/// One mip level of a surface as it is laid out: where it starts, its layout, the bits per element and the tiling address inputs.
struct Level {
    base: u32,
    surface: Surface,
    size: u64,
}

/// Where `level` of the surface at `s` is and how it is laid out, by the same rules `GX2CalcSurfaceSizeAndAlignment` used.
fn level_layout(g: &Guest, s: u32, level: u32) -> Option<Level> {
    let (dim, width, height, depth, levels, format) = (
        g.u32(s),
        g.u32(s + 4),
        g.u32(s + 8),
        g.u32(s + 0xc),
        g.u32(s + 0x10),
        g.u32(s + 0x14),
    );
    if level >= levels.max(1) || level > 13 || width == 0 {
        return None;
    }
    let base = match level {
        0 => g.u32(s + 0x24),
        1 => g.u32(s + 0x2c),
        n => g.u32(s + 0x2c).wrapping_add(g.u32(s + 0x3c + 4 * n)),
    };
    if base == 0 {
        return None;
    }
    let (bytes, compressed) = element(format);
    let mip = |v: u32| (v >> level).max(1);
    let slices = match dim {
        2 => mip(depth),
        3 => depth.max(6),
        4 | 5 | 7 => depth,
        _ => 1,
    };
    let info = tiling::surface_info(&tiling::Request {
        mode: TileMode::from_number(g.u32(s + 0x30)).unwrap_or(TileMode::LinearAligned),
        bits_per_element: bytes * 8,
        compressed,
        width: mip(width),
        height: if dim == 0 || dim == 4 { 1 } else { mip(height) },
        slices: slices.max(1),
        samples: 1 << g.u32(s + 0x18).min(3),
        level,
        cube: dim == 3,
        depth: g.u32(s + 0x1c) & 4 != 0,
        display: g.u32(s + 0x1c) & 8 != 0,
    });
    let samples = 1 << g.u32(s + 0x18).min(3);
    let (pipe_swizzle, bank_swizzle) = tiling::swizzles(g.u32(s + 0x34));
    Some(Level {
        base,
        surface: Surface {
            mode: info.mode,
            bits_per_element: bytes * 8,
            pitch: info.pitch,
            height: info.height,
            samples,
            depth: g.u32(s + 0x1c) & 4 != 0,
            pipe_swizzle,
            bank_swizzle,
        },
        size: info.size / u64::from(info.slices.max(1)),
    })
}

/// `GX2GetSurfaceMipPitch(surface, level)`: the pitch of a level in elements (level 0's is the surface's own).
fn mip_pitch(g: &Guest, s: u32, level: u32) -> u32 {
    if level == 0 {
        return g.u32(s + 0x3c);
    }
    level_layout(g, s, level).map_or(0, |l| l.surface.pitch)
}

/// Where tiling apertures are put in the guest's address space: a range nothing else uses (memory is committed when touched).
const APERTURES: (u32, u32) = (0xe800_0000, 0x0800_0000);

/// The tiling apertures the game has open, by handle.
#[derive(Default)]
pub struct Apertures {
    next: u32,
    open: BTreeMap<u32, Aperture>,
}

/// A linear copy of one slice of a surface level, at `address`, and where its elements are in the surface.
struct Aperture {
    address: u32,
    len: u32,
    base: u32,
    surface: Surface,
    slice: u32,
    endian: u32,
}

impl Aperture {
    /// Copies the surface into the aperture (`into_aperture`) or the aperture back into the surface, element by element.
    fn copy(&self, g: &Guest, into_aperture: bool) {
        let s = &self.surface;
        let bytes = (s.bits_per_element / 8).max(1) as usize;
        let mut element = vec![0u8; bytes];
        for y in 0..s.height {
            for x in 0..s.pitch {
                let linear = self.address + (y * s.pitch + x) * bytes as u32;
                let tiled = self.base.wrapping_add(tiling::address(s, x, y, self.slice, 0) as u32);
                let (from, to) = if into_aperture { (tiled, linear) } else { (linear, tiled) };
                g.read(from, &mut element);
                swap(&mut element, self.endian, bytes);
                g.write(to, &element);
            }
        }
    }
}

/// Turns the bytes of one element around as an endian swap mode says (0: none, 1: 8 in 16, 2: 8 in 32, 3: the format's own size).
fn swap(element: &mut [u8], endian: u32, bytes: usize) {
    let unit = match endian {
        1 => 2,
        2 => 4,
        3 if bytes == 2 => 2,
        3 if bytes >= 4 => 4,
        _ => return,
    };
    for chunk in element.chunks_exact_mut(unit) {
        chunk.reverse();
    }
}

impl Apertures {
    /// Opens an aperture on `slice` of `level` of the surface at `s`: (handle, address), or nothing for a level that is not there.
    fn open(&mut self, g: &Guest, s: u32, level: u32, slice: u32, endian: u32) -> Option<(u32, u32)> {
        let l = level_layout(g, s, level)?;
        let bytes = u64::from((l.surface.bits_per_element / 8).max(1));
        let len = (u64::from(l.surface.pitch) * u64::from(l.surface.height) * bytes).max(l.size);
        let len = u32::try_from(len.div_ceil(0x100) * 0x100).ok()?;
        // the lowest gap that fits, after the apertures open now
        let mut taken: Vec<(u32, u32)> = self.open.values().map(|a| (a.address, a.len)).collect();
        taken.sort_unstable();
        let mut at = APERTURES.0;
        for (address, used) in taken {
            if at.checked_add(len)? <= address {
                break;
            }
            at = at.max(address + used);
        }
        if u64::from(at) + u64::from(len) > u64::from(APERTURES.0) + u64::from(APERTURES.1) {
            return None;
        }
        self.next = self.next.wrapping_add(1).max(1);
        let a = Aperture {
            address: at,
            len,
            base: l.base,
            surface: l.surface,
            slice,
            endian,
        };
        g.fill(at, 0, len);
        a.copy(g, true);
        self.open.insert(self.next, a);
        Some((self.next, at))
    }

    /// Closes an aperture: its contents go back into the surface (the console's aperture is a view, so writes through it land there).
    fn close(&mut self, g: &Guest, handle: u32) {
        if let Some(a) = self.open.remove(&handle) {
            a.copy(g, false);
        }
    }
}

/// `GX2CalcSurfaceSizeAndAlignment(surface)`: resolves the tile mode and fills in the image size, mip size, alignment, pitch,
/// swizzle and the mip offsets, by the address library's rules for each level (the order of the steps is the console library's).
fn calc_surface(g: &Guest, s: u32) {
    let (dim, width, height, depth, asked, format, aa, usage) = (
        g.u32(s),
        g.u32(s + 4),
        g.u32(s + 8),
        g.u32(s + 0xc),
        g.u32(s + 0x10),
        g.u32(s + 0x14),
        g.u32(s + 0x18),
        g.u32(s + 0x1c),
    );
    let (bytes, compressed) = element(format);
    let is_depth = usage & 4 != 0;
    let is_color = usage & 2 != 0;
    let mut mode = g.u32(s + 0x30);
    let mut changed = false;
    if mode == 0 {
        if dim != 0 || aa != 0 || is_depth {
            mode = if dim != 2 || is_color {
                TileMode::TwoDThin1
            } else {
                TileMode::TwoDThick
            } as u32;
            changed = true;
        } else {
            mode = TileMode::LinearAligned as u32;
        }
    }
    let levels = asked.max(1).min(level_count(dim, width, height, depth, asked));
    let mut swizzle = g.u32(s + 0x34) & 0xff00ffff;
    let tiled = |m: u32| m >= TileMode::TwoDThin1 as u32 && m != 0x10;
    if tiled(mode) {
        swizzle |= 0xd0000;
    }
    let mut last = mode;
    let (mut prev_size, mut offset0) = (0u32, 0u32);
    let mut offsets = [0u32; 14];
    let (mut image_size, mut alignment, mut pitch) = (0, 0, 0);
    for level in 0..levels {
        let ask = |mode: u32| {
            let mip = |v: u32| (v >> level).max(1);
            let slices = match dim {
                0 => 1,
                2 => mip(depth),
                3 => depth.max(6),
                4 | 5 | 7 => depth,
                _ => 1,
            };
            tiling::surface_info(&tiling::Request {
                mode: TileMode::from_number(mode).unwrap_or(TileMode::LinearAligned),
                bits_per_element: bytes * 8,
                compressed,
                width: mip(width),
                height: if dim == 0 || dim == 4 { 1 } else { mip(height) },
                slices: slices.max(1),
                samples: 1 << aa,
                level,
                cube: dim == 3,
                depth: is_depth,
                display: usage & 8 != 0,
            })
        };
        let mut info = ask(mode);
        if level > 0 {
            let mut pad = 0;
            if tiled(last) && (info.mode as u32) < TileMode::TwoDThin1 as u32 {
                swizzle = (level << 16) | (swizzle & 0xff00ffff);
                last = info.mode as u32;
                if level > 1 {
                    pad = swizzle & 0xffff;
                }
            }
            let align = info.base_align.max(1);
            pad += (align - prev_size % align) % align;
            if level == 1 {
                offset0 = pad + prev_size;
            } else {
                offsets[level as usize - 1] = pad + prev_size + offsets[level as usize - 2];
            }
        } else {
            if changed && mode != info.mode as u32 {
                mode = info.mode as u32;
                info = ask(mode);
                if !tiled(mode) {
                    swizzle &= 0xff00ffff;
                }
                last = mode;
            }
            if changed && width < info.pitch_align && height < info.height_align {
                mode = if mode == TileMode::TwoDThick as u32 {
                    TileMode::OneDThick
                } else {
                    TileMode::OneDThin1
                } as u32;
                info = ask(mode);
                swizzle &= 0xff00ffff;
                last = mode;
            }
            image_size = info.size as u32;
            alignment = info.base_align;
            pitch = info.pitch;
        }
        prev_size = info.size as u32;
    }
    let mip_size = if levels <= 1 { 0 } else { prev_size + offsets[levels as usize - 2] };
    offsets[0] = offset0;
    g.put_u32(s + 0x20, image_size);
    g.put_u32(s + 0x28, mip_size);
    g.put_u32(s + 0x30, mode);
    g.put_u32(s + 0x34, swizzle);
    g.put_u32(s + 0x38, alignment);
    g.put_u32(s + 0x3c, pitch);
    g.put_u32(s + 0x10, levels);
    for (i, o) in offsets.iter().take(13).enumerate() {
        g.put_u32(s + 0x40 + 4 * i as u32, *o);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cafe::{Handler, Os, find};

    fn call(os: &mut Os, g: &Guest, name: &str, r: &[u32]) -> u32 {
        let mut c = crate::game::new_ctx(std::ptr::null_mut());
        c.r[3..3 + r.len()].copy_from_slice(r);
        let Some(Handler::Simple(h)) = find(name) else { panic!("{name}") };
        h(os, g, &mut c);
        c.r[3]
    }

    /// A tiled 2D RGBA8 surface as the picker's palette is: the aperture shows its texels in rows, and closing it puts them back.
    #[test]
    fn a_tiling_aperture_is_a_linear_view_of_a_tiled_surface() {
        let mut memory = crate::memory::GuestMemory::reserve().expect("guest memory");
        let g = Guest::new(memory.as_mut_ptr());
        let mut os = Os::new(&g);
        let (s, image, out) = (0x2000_0000, 0x2010_0000, 0x2000_1000);
        // 2D, 128 x 64, one level, RGBA8 (0x1a), used as a texture, tile mode 2D thin (4)
        for (at, v) in [(0, 1), (4, 128), (8, 64), (0xc, 1), (0x10, 1), (0x14, 0x1a), (0x1c, 1), (0x30, 4)] {
            g.put_u32(s + at, v);
        }
        call(&mut os, &g, "gx2:GX2CalcSurfaceSizeAndAlignment", &[s]);
        assert_eq!(g.u32(s + 0x30), 4, "stays tiled");
        g.put_u32(s + 0x24, image);
        let pitch = call(&mut os, &g, "gx2:GX2GetSurfaceMipPitch", &[s, 0]);
        assert_eq!(pitch, g.u32(s + 0x3c));
        assert_eq!(call(&mut os, &g, "gx2:GX2GetSurfaceFormatBits", &[0x1a]), 32);
        let (pipe_swizzle, bank_swizzle) = tiling::swizzles(g.u32(s + 0x34));
        let layout = Surface {
            mode: TileMode::TwoDThin1,
            bits_per_element: 32,
            pitch,
            height: 64,
            samples: 1,
            depth: false,
            pipe_swizzle,
            bank_swizzle,
        };
        let texel = |x: u32, y: u32| (y << 16) | x | 0x8000_0000;
        for y in 0..64 {
            for x in 0..128 {
                g.put_u32(image + tiling::address(&layout, x, y, 0, 0) as u32, texel(x, y));
            }
        }
        call(&mut os, &g, "gx2:GX2AllocateTilingApertureEx", &[s, 0, 0, 0, out, out + 4]);
        let (handle, view) = (g.u32(out), g.u32(out + 4));
        assert_ne!(handle, 0);
        assert_ne!(view, 0);
        for (x, y) in [(0, 0), (1, 0), (7, 3), (100, 37), (127, 63)] {
            assert_eq!(g.u32(view + (y * pitch + x) * 4), texel(x, y), "({x}, {y})");
        }
        // a write through the aperture reaches the surface when it is closed
        g.put_u32(view + (5 * pitch + 9) * 4, 0x1234_5678);
        call(&mut os, &g, "gx2:GX2FreeTilingAperture", &[handle]);
        assert_eq!(g.u32(image + tiling::address(&layout, 9, 5, 0, 0) as u32), 0x1234_5678);
        assert_eq!(g.u32(image + tiling::address(&layout, 10, 5, 0, 0) as u32), texel(10, 5));
        // 8-in-32 turns each texel's bytes around
        call(&mut os, &g, "gx2:GX2AllocateTilingApertureEx", &[s, 0, 0, 2, out, out + 4]);
        assert_eq!(g.u32(g.u32(out + 4) + 4), texel(1, 0).swap_bytes());
        call(&mut os, &g, "gx2:GX2FreeTilingAperture", &[g.u32(out)]);
        assert_eq!(g.u32(image + tiling::address(&layout, 1, 0, 0, 0) as u32), texel(1, 0));
    }
}
