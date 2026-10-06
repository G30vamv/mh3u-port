//! Spike 3d: find the game's shader programs in the memory of a running Cemu, where the vertex shaders (which are in none of the game's
//! files) must be. Starts Cemu on the game (only a process we started can be read), waits until you have a scene on screen, and
//! looks through the game's memory at every 256-byte boundary (GX2 aligns programs to that):
//!
//! - for the pixel shaders of the game's own pack, by exact match of their first bytes, which shows the scan works and says which of
//!   them the scene uses;
//! - for **vertex-like** programs, by structure: one that starts by calling its fetch shader, parses cleanly to its end with
//!   `mh3u_latte::census`, exports its results and does some arithmetic. Noise rarely passes that, and what does is kept apart by what
//!   it holds.
//!
//! What it finds is made from the game and goes to a folder that must not be in a git working tree.

use anyhow::{Context, Result, bail};
use mh3u_cemu::procmem::ProcMem;
use mh3u_dump::Dump;
use mh3u_latte::{Census, census};
use std::collections::{BTreeMap, HashMap};
use std::io::BufRead;
use std::path::Path;

/// GX2 aligns shader programs to this.
const ALIGN: usize = 256;
/// The longest vertex-like program looked at.
const WINDOW: usize = 256 << 10;
/// How many bytes of a known program must match.
const PREFIX: usize = 32;
/// The smallest vertex-like program kept: its calls and exports, and a couple of groups of arithmetic.
const SMALLEST: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A program from the game's pack, by its place in the pack.
    Known(usize),
    /// A program that starts by calling its fetch shader.
    Vertex,
}

pub struct Found {
    pub address: u64,
    pub kind: Kind,
    pub bytes: Vec<u8>,
    pub census: Option<Census>,
}

fn hash(bytes: &[u8]) -> u64 {
    // FNV-1a: only for naming files and telling copies apart
    bytes
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// The programs of the game's pack by their first bytes, to find them again in memory.
pub fn known_prefixes(programs: &[Vec<u8>]) -> HashMap<Vec<u8>, Vec<usize>> {
    let mut map: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
    for (i, p) in programs.iter().enumerate().filter(|(_, p)| p.len() >= PREFIX) {
        // a program that is the same bytes as an earlier one is not listed twice
        let same = map.get(&p[..PREFIX]).is_some_and(|list| list.iter().any(|&j| programs[j] == *p));
        if !same {
            map.entry(p[..PREFIX].to_vec()).or_default().push(i);
        }
    }
    map
}

/// Programs in one stretch of memory that starts at the address `base`, on the boundaries of the game's memory.
pub fn programs_in(base: u64, chunk: &[u8], known: &HashMap<Vec<u8>, Vec<usize>>, programs: &[Vec<u8>]) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    let mut off = (ALIGN - (base as usize % ALIGN)) % ALIGN;
    let mut free_from = 0usize; // what lies inside a program already found is not looked at again
    while off + 8 <= chunk.len() {
        if off < free_from {
            off += ALIGN;
            continue;
        }
        let window = &chunk[off..(off + WINDOW).min(chunk.len())];
        // of the known programs that start like this one, the longest that matches all the way
        let matched = window.get(..PREFIX).and_then(|p| known.get(p)).and_then(|list| {
            list.iter()
                .copied()
                .filter(|&i| window.starts_with(&programs[i]))
                .max_by_key(|&i| programs[i].len())
        });
        if let Some(i) = matched {
            out.push(Found {
                address: base + off as u64,
                kind: Kind::Known(i),
                bytes: Vec::new(),
                census: None,
            });
            free_from = off + programs[i].len();
        } else if let Some(w1) = window.get(4..8).map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")))
            && (w1 >> 23) & 0x7f == 0x13
            && let Ok(c) = census(window)
            && c.extent >= SMALLEST
            && c.exports > 0
            && c.alu_instructions >= 2
        {
            out.push(Found {
                address: base + off as u64,
                kind: Kind::Vertex,
                bytes: window[..c.extent].to_vec(),
                census: Some(c.clone()),
            });
            free_from = off + c.extent;
        }
        off += ALIGN;
    }
    out
}

fn scan(mem: &ProcMem, known: &HashMap<Vec<u8>, Vec<usize>>, programs: &[Vec<u8>]) -> Result<Vec<Found>> {
    let mut all = Vec::new();
    mem.for_each_chunk_in(0..u64::MAX, WINDOW, |addr, chunk| {
        // a program that starts in the overlap is seen again by the next chunk; keep it once
        let own = chunk.len().min(mh3u_cemu::procmem::CHUNK_LEN);
        all.extend(
            programs_in(addr, chunk, known, programs)
                .into_iter()
                .filter(|f| f.address < addr + own as u64),
        );
    })?;
    Ok(all)
}

pub fn run(dump: &Dump, cemu: &str, out: &Path, programs: &[Vec<u8>]) -> Result<()> {
    if std::path::absolute(out)?.ancestors().any(|p| p.join(".git").exists()) {
        bail!(
            "{} is inside a git working tree: what this writes is made from the game and must not go in a repository",
            out.display()
        );
    }
    std::fs::create_dir_all(out)?;
    let rpx = dump.base_executable()?;
    use std::os::unix::process::CommandExt;
    let mut child = std::process::Command::new(cemu)
        .process_group(0)
        .arg("-g")
        .arg(&rpx)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("starting {cemu}"))?;
    println!(
        "Cemu started (process {}). Get the game to a scene (a village, a quest) and press Enter here to scan; Enter again to scan again; q then Enter to stop.",
        child.id()
    );
    let known = known_prefixes(programs);
    let mut known_seen: BTreeMap<usize, u64> = BTreeMap::new();
    let mut vertex: BTreeMap<u64, Census> = BTreeMap::new();
    let mut mem = ProcMem::open(child.id()).context("opening Cemu's memory")?;
    for line in std::io::stdin().lock().lines() {
        if line?.trim().eq_ignore_ascii_case("q") {
            break;
        }
        let _ = mem.reopen();
        let started = std::time::Instant::now();
        let found = scan(&mem, &known, programs)?;
        let mut new_vertex = 0;
        for f in &found {
            match f.kind {
                Kind::Known(i) => {
                    known_seen.entry(i).or_insert(f.address);
                }
                Kind::Vertex => {
                    let h = hash(&f.bytes);
                    if let std::collections::btree_map::Entry::Vacant(e) = vertex.entry(h) {
                        e.insert(f.census.clone().unwrap_or_default());
                        new_vertex += 1;
                        std::fs::write(out.join(format!("vs-{h:016x}.bin")), &f.bytes)?;
                    }
                }
            }
        }
        println!(
            "scan took {:.0} s: {} of the pack's {} pixel shaders are in memory (so far); {} vertex-like programs found, {new_vertex} new",
            started.elapsed().as_secs_f64(),
            known_seen.len(),
            programs.len(),
            vertex.len(),
        );
        let mut total = Census::default();
        for c in vertex.values() {
            total.add(c);
        }
        if total.programs > 0 {
            println!(
                "  the vertex-like programs: {} instructions in {} groups, {} exports; {} two-operand and {} three-operand operations; control flow {:?}",
                total.alu_instructions,
                total.groups,
                total.exports,
                total.op2.len(),
                total.op3.len(),
                total.control_flow
            );
        }
    }
    let _ = child.kill();
    println!("done; {} vertex-like programs are in {}", vertex.len(), out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The control flow of a vertex shader that calls its fetch shader, runs one clause and exports and ends. The clause has two groups.
    fn vertex_like() -> Vec<u8> {
        [
            0u32,
            0x13 << 23, // call the fetch shader
            3,
            (8 << 26) | (1 << 18), // an ALU clause of two slots at slot 3, after the control flow
            0,
            (0x28 << 23) | (1 << 21), // export, end
            // two ALU groups of one instruction each (MOV, MUL), each last in its group
            1 << 16 | 1 << 31,
            25 << 7,
            2 << 16 | 1 << 31,
            1 << 7,
            // padding that makes it long enough to count
            0,
            0,
            0,
            0,
        ]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
    }

    #[test]
    fn a_vertex_like_program_in_noise_is_found_at_its_boundary_and_noise_is_not() {
        let mut memory = vec![0xa5u8; 4096];
        let program = vertex_like();
        memory[512..512 + program.len()].copy_from_slice(&program);
        let found = programs_in(0x1000_0000, &memory, &HashMap::new(), &[]);
        assert_eq!(found.len(), 1, "{:?}", found.iter().map(|f| f.address).collect::<Vec<_>>());
        assert_eq!((found[0].address, found[0].kind), (0x1000_0000 + 512, Kind::Vertex));
        assert!(
            programs_in(0, &vec![0u8; 4096], &HashMap::new(), &[]).is_empty(),
            "zeros are not programs"
        );
        // a chunk that starts off the boundary is searched on the boundaries of the memory
        let found = programs_in(0x1000_0000 + 512 - 212, &memory[300..], &HashMap::new(), &[]);
        assert_eq!(found.first().map(|f| f.address), Some(0x1000_0000 + 512));
    }

    /// The scan on the real thing (skips without the game's files): the 967 pixel shaders of the pack are put in 64 MB of random bytes
    /// on 256-byte boundaries, and every one is found as a known program, and nothing is taken for a vertex shader.
    #[test]
    fn every_program_of_the_games_pack_is_found_in_noise_and_nothing_else() {
        use mh3u_formats::{arc::Arc, gfd};
        let Some(dump) = Dump::from_env() else { return };
        let Some(path) = dump.content("nativeCafe/shaderArc.arc") else {
            return;
        };
        let data = std::fs::read(path).unwrap();
        let arc = Arc::parse(&data).unwrap();
        let entry = arc.entries.iter().find(|e| e.name == "shaderPack").unwrap();
        let pack = arc.read(entry).unwrap();
        let mut programs = Vec::new();
        for file in gfd::parse_pack(&pack).unwrap() {
            let p = file.block(gfd::PIXEL_SHADER_PROGRAM).unwrap();
            programs.push(p[..census(p).unwrap().extent].to_vec());
        }
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut memory: Vec<u8> = (0..64 << 20)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        let mut at = 0usize;
        for (n, p) in programs.iter().enumerate() {
            let target = (n * (memory.len() - 300_000) / programs.len()) & !(ALIGN - 1);
            at = at.max(target);
            memory[at..at + p.len()].copy_from_slice(p);
            at = (at + p.len()).next_multiple_of(ALIGN);
        }
        let known = known_prefixes(&programs);
        let found = programs_in(0, &memory, &known, &programs);
        // different programs of the pack, by their bytes: some of the 967 are the same program twice
        let different: std::collections::BTreeSet<&Vec<u8>> = programs.iter().collect();
        let found_different: std::collections::BTreeSet<&Vec<u8>> = found
            .iter()
            .filter_map(|f| if let Kind::Known(i) = f.kind { Some(&programs[i]) } else { None })
            .collect();
        let vertex = found.iter().filter(|f| f.kind == Kind::Vertex).count();
        assert_eq!((found_different.len(), vertex), (different.len(), 0), "{} placed", programs.len());
        // and every one that was placed is found where it was put (the copies too)
        assert_eq!(found.len(), programs.len());
    }
}
