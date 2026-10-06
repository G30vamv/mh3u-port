//! Explorers for the game's files, for the port's groundwork. They read the owner's own dump (`--game-dir`, or the `MH3U_GAME_DIR`
//! environment variable) and print; they change nothing.

mod codegen_bench;

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
    /// Count the instructions of the executable's code by kind, and the ones that are not known.
    Coverage {
        game_dir: PathBuf,
        /// How many unknown words to show.
        #[arg(long, default_value_t = 20)]
        unknown: usize,
    },
    /// Spike 4: translate a sample of the game's functions into Rust or C, only to time how long that takes to compile. The output is
    /// made from the game and is refused inside a git working tree.
    CodegenBench {
        game_dir: PathBuf,
        /// Where to write the generated project (outside any repository).
        out: PathBuf,
        /// `rust` or `c`.
        #[arg(long, default_value = "rust")]
        lang: String,
        /// How many functions to translate.
        #[arg(long, default_value_t = 2000)]
        functions: usize,
        /// Functions per generated file.
        #[arg(long, default_value_t = 100)]
        per_file: usize,
    },
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
        Tool::CodegenBench {
            game_dir,
            out,
            lang,
            functions,
            per_file,
        } => {
            let data = read(&dump(&game_dir)?.executable()?)?;
            let text = rpx::section_at(&data, TEXT_ADDRESS)?;
            let words: Vec<u32> = text.as_chunks::<4>().0.iter().map(|w| u32::from_be_bytes(*w)).collect();
            let lang = match lang.as_str() {
                "rust" => codegen_bench::Lang::Rust,
                "c" => codegen_bench::Lang::C,
                other => anyhow::bail!("--lang is rust or c, not {other}"),
            };
            let r = codegen_bench::run(&words, TEXT_ADDRESS, functions, lang, &out, per_file)?;
            println!(
                "{} functions, {} instructions ({} placeholders), {} lines, {} bytes, {} files",
                r.functions, r.instructions, r.placeholders, r.lines, r.bytes, r.files
            );
            Ok(())
        }
        Tool::Coverage { game_dir, unknown } => coverage(&dump(&game_dir)?, unknown),
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

/// Where the executable's code is loaded.
const TEXT_ADDRESS: u32 = 0x0200_0020;

fn coverage(dump: &Dump, show_unknown: usize) -> Result<()> {
    use mh3u_ppc::{Class, decode, spr, spr_name};
    use std::collections::BTreeMap;
    let data = read(&dump.executable()?)?;
    let text = rpx::section_at(&data, TEXT_ADDRESS)?;
    let words: Vec<u32> = text.as_chunks::<4>().0.iter().map(|w| u32::from_be_bytes(*w)).collect();
    let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_class: BTreeMap<Class, usize> = BTreeMap::new();
    let mut sprs: BTreeMap<String, usize> = BTreeMap::new();
    let mut unknown: Vec<(u32, u32)> = Vec::new();
    let mut unknown_count = 0usize;
    let mut unknown_opcodes: BTreeMap<u32, usize> = BTreeMap::new();
    let mut zero = 0usize;
    // how the branches go: direct calls and jumps, returns, and the ones through a register
    let (mut calls, mut jumps, mut conditional, mut returns, mut conditional_returns, mut indirect_jumps, mut indirect_calls) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for (i, &word) in words.iter().enumerate() {
        let address = TEXT_ADDRESS + (i as u32) * 4;
        match decode(word) {
            Some(d) => {
                *by_name.entry(d.name).or_default() += 1;
                *by_class.entry(d.class).or_default() += 1;
                let link = word & 1 != 0;
                let always = (word >> 21) & 0x14 == 0x14;
                match (d.name, link) {
                    ("b", true) => calls += 1,
                    ("b", false) => jumps += 1,
                    ("bc", _) => conditional += 1,
                    ("bclr", false) if always => returns += 1,
                    ("bclr", _) if !always => conditional_returns += 1,
                    ("bclr", true) | ("bcctr", true) => indirect_calls += 1,
                    ("bcctr", false) => indirect_jumps += 1,
                    _ => {}
                }
                if d.name == "mfspr" || d.name == "mtspr" {
                    let n = spr(word);
                    let name = spr_name(n).map_or_else(|| format!("spr {n}"), str::to_string);
                    *sprs.entry(format!("{} {name}", d.name)).or_default() += 1;
                }
            }
            None if word == 0 => zero += 1,
            None => {
                unknown_count += 1;
                *unknown_opcodes.entry(word >> 26).or_default() += 1;
                if unknown.len() < show_unknown {
                    unknown.push((address, word));
                }
            }
        }
    }
    let total = words.len();
    let known: usize = by_class.values().sum();
    println!("code: {} bytes, {total} words from {TEXT_ADDRESS:#x}", text.len());
    println!(
        "  decoded {known} ({:.2}%), zero words {zero}, unknown {unknown_count}",
        100.0 * known as f64 / total as f64
    );
    println!("  {} different instructions are used", by_name.len());
    println!("\nby kind:");
    let mut kinds: Vec<_> = by_class.iter().collect();
    kinds.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (class, n) in kinds {
        println!("  {class:<10?} {n:>9} {:>6.2}%", 100.0 * *n as f64 / total as f64);
    }
    let count = |names: &[&str]| names.iter().map(|n| by_name.get(n).copied().unwrap_or(0)).sum::<usize>();
    println!("\nthe ones that cost a translator something:");
    println!(
        "  paired singles (ps_*, psq_*): {}",
        by_class.get(&Class::Paired).copied().unwrap_or(0)
    );
    println!("  branches to a register (bclr, bcctr): {}", count(&["bclr", "bcctr"]));
    println!("  reserved loads and stores (lwarx, stwcx.): {}", count(&["lwarx", "stwcx."]));
    println!(
        "  load/store multiple and string (lmw, stmw, lswi, lswx, stswi, stswx): {}",
        count(&["lmw", "stmw", "lswi", "lswx", "stswi", "stswx"])
    );
    println!(
        "  cache control: {}   sync/isync/eieio: {}",
        by_class.get(&Class::Cache).copied().unwrap_or(0),
        by_class.get(&Class::Sync).copied().unwrap_or(0)
    );
    println!(
        "  system (sc, traps, rfi): {}   64-bit only: {}",
        by_class.get(&Class::System).copied().unwrap_or(0),
        by_class.get(&Class::Only64).copied().unwrap_or(0)
    );
    println!(
        "  floating point status register: {}",
        count(&["mtfsb0", "mtfsb1", "mtfsf", "mtfsfi", "mffs", "mcrfs"])
    );
    println!("\nhow the branches go:");
    println!("  calls (bl): {calls}   jumps (b): {jumps}   conditional (bc, including loops): {conditional}");
    println!("  returns (blr): {returns}   conditional returns: {conditional_returns}");
    println!(
        "  indirect jumps (bctr: jump tables, tail calls): {indirect_jumps}   indirect calls (bctrl, blrl: function pointers, virtual calls): {indirect_calls}"
    );
    println!("\nspecial registers:");
    for (k, n) in &sprs {
        println!("  {k:<22} {n:>8}");
    }
    println!("\nmost used instructions:");
    let mut top: Vec<_> = by_name.iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (name, n) in top.iter().take(25) {
        println!("  {name:<12} {n:>9}");
    }
    println!("\nleast used (down to 1):");
    for (name, n) in top.iter().rev().take(25) {
        println!("  {name:<12} {n:>9}");
    }
    if unknown_count > 0 {
        println!("\nunknown words by opcode (first six bits):");
        for (op, n) in &unknown_opcodes {
            println!("  {op:>2}: {n}");
        }
        println!("first unknown words:");
        for (address, word) in &unknown {
            println!("  {address:#010x}  {word:08x}");
        }
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
