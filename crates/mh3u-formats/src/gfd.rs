//! GX2 shader files (`Gfx2`, "GFD"): what Nintendo's tools write for a shader or a texture, a header and a list of blocks, and the
//! game's pack of them (`GKP`): a short header and the files one after the other. All values are big-endian; the shader programs
//! inside the blocks are the GPU's own words (see the `mh3u-latte` crate).

use anyhow::{Result, bail};

const GFD_MAGIC: &[u8; 4] = b"Gfx2";
const BLOCK_MAGIC: &[u8; 4] = b"BLK{";
const PACK_MAGIC: &[u8; 4] = b"\0GKP";
/// Where a pack's files start, after its header (magic, version, count, and four more words).
const PACK_HEADER: usize = 0x1c;
/// The kinds of block that hold shaders.
pub const VERTEX_SHADER_HEADER: u32 = 3;
pub const VERTEX_SHADER_PROGRAM: u32 = 5;
pub const PIXEL_SHADER_HEADER: u32 = 6;
pub const PIXEL_SHADER_PROGRAM: u32 = 7;
const END_OF_FILE: u32 = 1;
/// Each file of the pack is followed by eight bytes (two words: a number that differs from file to file, and a second one) that this does
/// not use.
const FOOTER: usize = 8;
/// The most blocks one file may have, so a made-up file cannot make a list of millions.
const MAX_BLOCKS: usize = 64;

fn be32(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(o..o.checked_add(4)?)?.try_into().ok()?))
}

/// One block of a file: its kind and its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block<'a> {
    pub kind: u32,
    pub data: &'a [u8],
}

/// One shader or texture file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gfd<'a> {
    pub blocks: Vec<Block<'a>>,
}

impl<'a> Gfd<'a> {
    /// The first block of this kind.
    pub fn block(&self, kind: u32) -> Option<&'a [u8]> {
        self.blocks.iter().find(|b| b.kind == kind).map(|b| b.data)
    }
}

/// Read the file that starts at `at`; the second value is where the next thing starts.
pub fn parse_file(data: &[u8], at: usize) -> Result<(Gfd<'_>, usize)> {
    if data.get(at..at.saturating_add(4)) != Some(GFD_MAGIC) {
        bail!("not a GX2 file at {at:#x}");
    }
    let header = be32(data, at + 4)
        .filter(|&h| (0x10..=0x100).contains(&h))
        .ok_or_else(|| anyhow::anyhow!("odd header size"))?;
    let mut p = at + header as usize;
    let mut blocks = Vec::new();
    loop {
        if blocks.len() >= MAX_BLOCKS {
            bail!("more than {MAX_BLOCKS} blocks");
        }
        if data.get(p..p.saturating_add(4)) != Some(BLOCK_MAGIC) {
            bail!("a block does not start where the last one ended ({p:#x})");
        }
        let (Some(hdr), Some(kind), Some(size)) = (be32(data, p + 4), be32(data, p + 16), be32(data, p + 20)) else {
            bail!("truncated block header");
        };
        if !(0x20..=0x100).contains(&hdr) {
            bail!("odd block header size");
        }
        let start = p + hdr as usize;
        let end = start.checked_add(size as usize).filter(|&e| e <= data.len());
        let Some(end) = end else {
            bail!("block of {size} bytes runs past the file")
        };
        if kind == END_OF_FILE {
            return Ok((Gfd { blocks }, end));
        }
        blocks.push(Block {
            kind,
            data: &data[start..end],
        });
        p = end;
    }
}

/// The files of a pack, in order. Checked against the count the pack gives.
pub fn parse_pack(data: &[u8]) -> Result<Vec<Gfd<'_>>> {
    if data.get(..4) != Some(PACK_MAGIC) {
        bail!("not a GKP pack");
    }
    let count = be32(data, 8).ok_or_else(|| anyhow::anyhow!("truncated pack"))? as usize;
    let mut files = Vec::new();
    let mut at = PACK_HEADER;
    while files.len() < count {
        let (file, next) = parse_file(data, at)?;
        files.push(file);
        at = next + FOOTER;
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(kind: u32, body: &[u8]) -> Vec<u8> {
        let mut b = b"BLK{".to_vec();
        for v in [0x20u32, 1, 0, kind, body.len() as u32, 0, 0] {
            b.extend(v.to_be_bytes());
        }
        b.extend(body);
        b
    }

    fn file(blocks: &[(u32, &[u8])]) -> Vec<u8> {
        let mut f = b"Gfx2".to_vec();
        f.extend(0x20u32.to_be_bytes());
        f.resize(0x20, 0);
        for (k, body) in blocks {
            f.extend(block(*k, body));
        }
        f.extend(block(END_OF_FILE, b""));
        f
    }

    #[test]
    fn a_pack_is_its_files_in_order() {
        let mut pack = b"\0GKP".to_vec();
        pack.extend(4u32.to_be_bytes());
        pack.extend(2u32.to_be_bytes());
        pack.resize(PACK_HEADER, 0);
        pack.extend(file(&[(PIXEL_SHADER_HEADER, b"head"), (PIXEL_SHADER_PROGRAM, b"program!")]));
        pack.extend([0, 0, 0x2b, 0x1c, 0xff, 0xff, 0xff, 0xff]); // the footer of a file
        pack.extend(file(&[(VERTEX_SHADER_PROGRAM, b"vtx")]));
        pack.extend([0u8; FOOTER]);
        let files = parse_pack(&pack).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].block(PIXEL_SHADER_PROGRAM), Some(&b"program!"[..]));
        assert_eq!(files[1].block(VERTEX_SHADER_PROGRAM), Some(&b"vtx"[..]));
        assert_eq!(files[1].block(PIXEL_SHADER_PROGRAM), None);
    }

    #[test]
    fn files_that_are_cut_short_or_made_up_are_errors() {
        let f = file(&[(PIXEL_SHADER_PROGRAM, b"program!")]);
        for cut in 0..f.len() {
            assert!(parse_file(&f[..cut], 0).is_err(), "cut at {cut}");
        }
        assert!(parse_pack(b"\0GKP").is_err());
        assert!(parse_pack(b"nope").is_err());
        // a count that asks for more files than there are
        let mut pack = b"\0GKP".to_vec();
        pack.extend(4u32.to_be_bytes());
        pack.extend(5u32.to_be_bytes());
        pack.resize(PACK_HEADER, 0);
        pack.extend(&f);
        pack.extend([0u8; FOOTER]);
        assert!(parse_pack(&pack).is_err());
    }
}
