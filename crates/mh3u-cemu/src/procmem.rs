//! Read-only access to another process's memory on Linux, through `/proc/<pid>/mem`.
//!
//! The kernel only allows this for a process we started (or with extra privileges; see `kernel.yama.ptrace_scope`). Linux only: on other systems this crate is empty.

use std::{fs::File, io, os::unix::fs::FileExt};

/// One line of `/proc/<pid>/maps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub readable: bool,
    pub writable: bool,
    pub private: bool,
}

/// Parse the text of a `/proc/<pid>/maps` file.
pub fn parse_maps(text: &str) -> Vec<Region> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (range, perms) = (parts.next()?, parts.next()?);
            let (start, end) = range.split_once('-')?;
            let perms = perms.as_bytes();
            Some(Region {
                start: u64::from_str_radix(start, 16).ok()?,
                end: u64::from_str_radix(end, 16).ok()?,
                readable: perms.first() == Some(&b'r'),
                writable: perms.get(1) == Some(&b'w'),
                private: perms.get(3) == Some(&b'p'),
            })
        })
        .collect()
}

/// How far apart the chunks of `for_each_chunk` start.
pub const CHUNK_LEN: usize = 16 << 20;

pub struct ProcMem {
    pid: u32,
    file: File,
}

impl ProcMem {
    /// Open a process's memory. Fails with `PermissionDenied` unless we are allowed to read it.
    pub fn open(pid: u32) -> io::Result<ProcMem> {
        Ok(ProcMem {
            pid,
            file: File::open(format!("/proc/{pid}/mem"))?,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Open the memory file again. A handle stays tied to the memory the process had when it was opened, so after the
    /// process replaces itself (a launcher script that starts the real program) the old handle reads nothing.
    pub fn reopen(&mut self) -> io::Result<()> {
        self.file = File::open(format!("/proc/{}/mem", self.pid))?;
        Ok(())
    }

    pub fn regions(&self) -> io::Result<Vec<Region>> {
        Ok(parse_maps(&std::fs::read_to_string(format!("/proc/{}/maps", self.pid))?))
    }

    /// Read exactly `len` bytes at `addr`.
    pub fn read(&self, addr: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.file.read_exact_at(&mut buf, addr)?;
        Ok(buf)
    }

    /// Call `f(address, bytes)` for every chunk of readable, writable memory. Chunks overlap by `overlap` bytes, so something up to that
    /// long that straddles a boundary is whole in one of them; `f` should only report things that start in the first
    /// `CHUNK_LEN` bytes of one. Unreadable chunks are skipped.
    pub fn for_each_chunk(&self, overlap: usize, f: impl FnMut(u64, &[u8])) -> io::Result<()> {
        self.for_each_chunk_in(0..u64::MAX, overlap, f)
    }

    /// Like `for_each_chunk`, but only the parts of memory inside `within`.
    pub fn for_each_chunk_in(&self, within: std::ops::Range<u64>, overlap: usize, mut f: impl FnMut(u64, &[u8])) -> io::Result<()> {
        let mut buf = vec![0u8; CHUNK_LEN + overlap];
        for region in self.regions()?.into_iter().filter(|r| r.readable && r.writable) {
            let (start, end) = (region.start.max(within.start), region.end.min(within.end));
            let mut addr = start;
            while addr < end {
                let want = ((end - addr) as usize).min(CHUNK_LEN + overlap);
                if self.file.read_exact_at(&mut buf[..want], addr).is_ok() {
                    f(addr, &buf[..want]);
                }
                addr += CHUNK_LEN as u64;
            }
        }
        Ok(())
    }

    /// Where `pattern` occurs in `chunk` (a piece of memory read with `for_each_chunk_in`).
    pub fn matches_in(chunk: &[u8], pattern: &[u8]) -> Vec<usize> {
        memchr::memmem::find_iter(chunk, pattern).collect()
    }

    /// Find every address where `pattern` occurs in readable, writable memory. Unreadable chunks are skipped.
    pub fn scan(&self, pattern: &[u8]) -> io::Result<Vec<u64>> {
        const CHUNK: usize = 16 << 20;
        let finder = memchr::memmem::Finder::new(pattern);
        let mut hits = Vec::new();
        let mut buf = vec![0u8; CHUNK + pattern.len()];
        for region in self.regions()?.into_iter().filter(|r| r.readable && r.writable) {
            let mut addr = region.start;
            while addr < region.end {
                // Read a little past the chunk so a match straddling the boundary is still seen.
                let want = ((region.end - addr) as usize).min(CHUNK + pattern.len() - 1);
                let Ok(()) = self.file.read_exact_at(&mut buf[..want], addr) else {
                    addr += CHUNK as u64;
                    continue;
                };
                for hit in finder.find_iter(&buf[..want]) {
                    if hit < CHUNK {
                        hits.push(addr + hit as u64);
                    }
                }
                addr += CHUNK as u64;
            }
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_maps_lines() {
        let text = "\
720856000000-7208a4000000 rw-p 00000000 00:00 0
7ffc519ac000-7ffc519cf000 rw-p 00000000 00:00 0                          [stack]
741e2f624000-741e2f79f000 r-xp 00026000 00:2b 123                        /usr/lib/libc.so.6
ffffffffff600000-ffffffffff601000 --xp 00000000 00:00 0                  [vsyscall]
";
        let r = parse_maps(text);
        assert_eq!(r.len(), 4);
        assert_eq!((r[0].start, r[0].end), (0x720856000000, 0x7208a4000000));
        assert!(r[0].readable && r[0].writable && r[0].private);
        assert!(r[2].readable && !r[2].writable);
        assert!(!r[3].readable);
    }

    #[test]
    fn scans_its_own_memory_across_a_chunk_boundary() {
        // Our own memory is always readable. Make a unique pattern and find it again.
        let mut data = vec![0u8; 40 << 20];
        let pattern = b"mh3u-procmem-test-pattern-\x01\x02\x03";
        // Place it so it straddles the 16 MiB chunk boundary relative to the start of its mapping.
        let at = (16 << 20) - 5;
        data[at..at + pattern.len()].copy_from_slice(pattern);
        let mem = ProcMem::open(std::process::id()).unwrap();
        let hits = mem.scan(pattern).unwrap();
        let expected = data.as_ptr() as u64 + at as u64;
        assert!(hits.contains(&expected), "expected {expected:#x} in {hits:x?}");
        assert_eq!(mem.read(expected, pattern.len()).unwrap(), pattern);
        drop(data);
    }
}
