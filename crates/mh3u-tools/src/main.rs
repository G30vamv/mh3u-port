//! Explorers for the game's files, for the port's groundwork. They read the owner's own dump (`--game-dir`, or the `MH3U_GAME_DIR`
//! environment variable) and print; they change nothing.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mh3u_dump::{Dump, expand_home};
use mh3u_formats::{arc::Arc, gmd, rpx};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Tool,
}

#[derive(Subcommand)]
enum Tool {
    /// Check that a folder holds the game, and say where its files are.
    Dump { game_dir: PathBuf },
    /// List the sections of the game's executable.
    Sections { game_dir: PathBuf },
    /// List what the executable imports from the system libraries, by library (the size of the runtime a port would owe).
    Imports {
        game_dir: PathBuf,
        /// Print every name, not only the counts.
        #[arg(long)]
        names: bool,
    },
    /// List the entries of an archive.
    Arcls { arc: PathBuf },
    /// Extract every entry of an archive as OUTDIR/NAME.TYPEHASH.
    Arcx { arc: PathBuf, outdir: PathBuf },
    /// Print every string of a text table, or only the given indexes.
    Gmd { file: PathBuf, ids: Vec<usize> },
    /// Find byte patterns (hex) in every decompressed archive entry under a folder.
    Arcsearch {
        dir: PathBuf,
        #[arg(required = true)]
        patterns: Vec<String>,
    },
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| path.display().to_string())
}

fn dump(game_dir: &Path) -> Result<Dump> {
    Dump::locate(&expand_home(&game_dir.to_string_lossy()))
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Tool::Dump { game_dir } => {
            let d = dump(&game_dir)?;
            println!("game:       {}", d.base.display());
            match &d.update {
                Some(u) => println!("update:     {}", u.display()),
                None => println!("update:     none found beside the game"),
            }
            println!("executable: {}", d.executable()?.display());
            Ok(())
        }
        Tool::Sections { game_dir } => sections(&dump(&game_dir)?),
        Tool::Imports { game_dir, names } => imports(&dump(&game_dir)?, names),
        Tool::Arcls { arc } => {
            for e in Arc::parse(&read(&arc)?)?.entries {
                println!("{:08x} {:>9} {:>9}  {}", e.type_hash, e.compressed_size, e.size, e.name);
            }
            Ok(())
        }
        Tool::Arcx { arc, outdir } => {
            let data = read(&arc)?;
            let arc = Arc::parse(&data)?;
            for e in &arc.entries {
                let path = outdir.join(format!("{}.{:08x}", e.name.replace('\\', "/"), e.type_hash));
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&path, arc.read(e)?)?;
            }
            println!("extracted {} entries", arc.entries.len());
            Ok(())
        }
        Tool::Gmd { file, ids } => {
            let strings = gmd::parse(&read(&file)?)?;
            if ids.is_empty() {
                for (i, s) in strings.iter().enumerate() {
                    println!("{i:5} {s}");
                }
            } else {
                for &i in &ids {
                    println!("{i:5} {}", strings.get(i).map_or("<out of range>", String::as_str));
                }
            }
            Ok(())
        }
        Tool::Arcsearch { dir, patterns } => arcsearch(&dir, &patterns),
    }
}

fn sections(dump: &Dump) -> Result<()> {
    let data = read(&dump.executable()?)?;
    println!(
        "{:>3} {:<22} {:<12} {:>10} {:>10} {:>10}  flags",
        "#", "name", "kind", "address", "file", "bytes"
    );
    for s in rpx::sections(&data)? {
        println!(
            "{:>3} {:<22} {:<12} {:>#10x} {:>#10x} {:>10}  {:#010x}{}",
            s.index,
            s.name,
            s.kind_name(),
            s.addr,
            s.offset,
            s.size,
            s.flags,
            if s.compressed() { " zlib" } else { "" }
        );
    }
    Ok(())
}

fn imports(dump: &Dump, names: bool) -> Result<()> {
    let data = read(&dump.executable()?)?;
    let all = rpx::imports(&data)?;
    let mut libraries: Vec<&str> = all.iter().map(|i| i.library.as_str()).collect();
    libraries.dedup();
    libraries.sort_unstable();
    libraries.dedup();
    let (mut functions, mut datas) = (0, 0);
    for lib in libraries {
        let mine: Vec<_> = all.iter().filter(|i| i.library == lib).collect();
        let f = mine.iter().filter(|i| i.kind == rpx::ImportKind::Function).count();
        println!("{lib:<16} {f:>5} functions {:>4} data", mine.len() - f);
        functions += f;
        datas += mine.len() - f;
        if names {
            for i in mine {
                println!(
                    "    {} {}",
                    if i.kind == rpx::ImportKind::Function { "fn  " } else { "data" },
                    i.name
                );
            }
        }
    }
    println!("{:<16} {functions:>5} functions {datas:>4} data", "all");
    Ok(())
}

fn hex_bytes(h: &str) -> Result<Vec<u8>> {
    let h: String = h.chars().filter(|c| !c.is_whitespace()).collect();
    anyhow::ensure!(
        !h.is_empty() && h.len().is_multiple_of(2),
        "a pattern is an even number of hex digits: {h}"
    );
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).with_context(|| format!("not hex: {h}")))
        .collect()
}

fn arcsearch(dir: &Path, patterns: &[String]) -> Result<()> {
    let pats: Vec<Vec<u8>> = patterns.iter().map(|h| hex_bytes(h)).collect::<Result<_>>()?;
    let mut stack = vec![expand_home(&dir.to_string_lossy())];
    while let Some(dir) = stack.pop() {
        for ent in std::fs::read_dir(&dir)? {
            let path = ent?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "arc") {
                let data = std::fs::read(&path)?;
                let Ok(arc) = Arc::parse(&data) else { continue };
                for e in &arc.entries {
                    let Ok(body) = arc.read(e) else { continue };
                    for (pi, pat) in pats.iter().enumerate() {
                        for (off, _) in body.windows(pat.len()).enumerate().filter(|(_, w)| *w == &pat[..]) {
                            println!("pat{pi} {} :: {} ({:08x}) @ {off:#x}", path.display(), e.name, e.type_hash);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
