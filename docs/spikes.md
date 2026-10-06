# Spikes: what has been measured

Each spike answers one question about whether the port is worth doing. The numbers come from the owner's own dump (the US game with its
update); nothing from the game is kept here beyond counts and the names of the system libraries it uses.

## 1. Imports: how big is the runtime we would owe? (done)

`mh3u-tools imports <dump>` lists what the executable takes from the Wii U's system libraries, found through its symbol table.

| Library | Functions | What it is |
|---|---:|---|
| `coreinit` | 146 (+10 data) | the operating system: threads, memory, files, time, cache, atomics |
| `gx2` | 101 | graphics |
| `snd_core`, `snd_user` | 37 + 37 | audio |
| `padscore` | 29 | Wii remote and Pro controller |
| `vpad` | 20 | the gamepad (screen, buttons, touch, motion) |
| `proc_ui` | 7 | the home-button and foreground/background protocol |
| `nn_save` | 9 | saves |
| `h264` | 12 | video decoding |
| `nsyshid`, `mic`, `usb_mic`, `dmae`, `sysapp` | 9, 8, 9, 2, 2 | USB devices, microphones, a memory copy engine, system app |
| `nsysnet`, `nlibcurl`, `nn_ac`, `nn_act`, `nn_fp`, `nn_uds`, `nn_ndm`, `nn_acp` | 29, 24, 13, 17, 32, 28, 5, 2 | network, accounts, friends, local multiplayer: online play |
| **all, 22 libraries** | **578 (+10 data)** | |

Reading it:

- The number is **small** for a game of this size. The libraries that would have to really work (`coreinit`, `gx2`, audio, `vpad`,
  `padscore`, `proc_ui`, `nn_save`) come to about 390 functions. The ~190 network, account and friends functions (`nsysnet`,
  `nlibcurl`, `nn_*` but `nn_save`) can answer "not available", since the game's servers are gone and the port has no online play.
  Whether the game copes with those answers is not known and is the first thing to test.
- `gx2` has 101 functions. Cemu implements all of them, so there is a reference; the real work is the translation into a PC graphics API.
- Not counted: what the game does with the *numbers* of those calls (state it expects to persist, timing, the order of calls).
- `h264` means the game has video (a movie) the port has to decode or skip.

## 2. Instruction coverage (done)

`mh3u-tools coverage <dump>` decodes every word of the code section (`.text`, at `0x02000020`: 7.9 MB in the file, **18,441,092 bytes
once unpacked: 4,610,273 instructions**) with `mh3u-ppc`, a decoder for the Wii U's PowerPC (Espresso) written from the architecture
book.

- **Coverage: 4,610,267 of 4,610,273 words decode (99.9999%).** 134 different instructions are used. The six that do not decode are the
  words `00400000` and `00600000` at `0x0312cf14` to `0x0312d244`, near the end of the section: data between functions, not code.
- **By kind:** integer arithmetic 20.9%, branches 18.6%, loads 14.2%, floating point loads and stores 11.6%, stores 10.2%, logic 9.2%,
  compares 5.2%, floating point arithmetic 3.9%, moves to and from special registers 3.6%, rotates 1.9%, **paired singles 0.37%
  (17,209)**, sync 0.33%, condition register logic 0.08%, cache control 194 (0.004%), `tw` 2.
- **Not used at all:** the 64-bit instructions, the floating point status register (`mtfsf`, `mffs` and the like: 0), `rfi`, `sc`,
  and no special register but the link register (LR) and count register (CTR). In particular the game's code never sets the quantization
  registers (GQR0-7) that paired singles use for `psq_l` and `psq_st`, so it relies on what the system leaves in them: the runtime has to
  set them the way the console does, and that value is not in the game.
- **Costly for a translator:** load and store multiple/string (`lmw`, `stmw`, `lswi`, ...) 26,911, mostly function entry and exit;
  reserved loads and stores (`lwarx`, `stwcx.`) 388, the lock operations; paired singles 17,209, two floats at a time, needing a
  translator that handles pairs. `sync`/`isync`/`eieio` 15,189 can be ignored on one thread and mapped to fences otherwise.
- **How the branches go:** calls (`bl`) 305,978; direct jumps 90,129; conditional branches 298,778 (loops and ifs); returns 115,463
  plus 11,614 conditional returns; **indirect jumps (`bctr`) 4,822, and indirect calls through a register (`bctrl`, `blrl`) 30,605**.
  Direct branches are easy. The 4,822 indirect jumps are jump tables (switch statements) and tail calls, and each one has to be
  resolved to the list of places it can go; that is the known hard part of a static recompiler and it is a few thousand, not tens of
  thousands. The 30,605 indirect calls (function pointers and virtual calls) go through a table from the game's own addresses to the
  translated functions, filled at start: they cost a lookup each and need no analysis.

Reading it:

- **The CPU side is small.** 134 instructions in 15 kinds, many of them variations of one another, with a reference interpreter to
  test the translator against. That is weeks of work, not a year (an estimate, not a measurement).
- **The size is not small.** 4.6 million instructions translate to many millions of lines of generated code. Whether that compiles in
  a reasonable time, and as what (Rust, or C for a C compiler the owner must then have), is **not known**, and `pc-port.md` plans to
  generate Rust. A spike to settle it is needed before going on: translate a few thousand functions, compile them as Rust and as C,
  and time both (spike 4 below).
- About 127,000 returns means on the order of a hundred thousand functions: the translated program is split into many files, which the
  owner's machine builds once, on first start.

## 3. One frame (not done)

The question is whether the graphics layer is reachable: take one known draw call from a Cemu capture and say what it needs.

## 4. Does the generated code compile in a reasonable time? (done: as C yes, as one Rust crate no)

`mh3u-tools codegen-bench <dump> <out> --lang rust|c --functions N` translates an even sample of the game's functions (found as the
targets of direct calls) into Rust or C in the shape a recompiler would give them, to be built and timed. **It is not a translator**:
98% of the instructions get real code (integer arithmetic, rotates, compares, loads and stores, floating point, branches by block) and
the rest a placeholder call; nothing checks that it behaves like the game. It refuses to write inside a git working tree, because
its output is made from the game. The runs below are on one machine (24 threads, 31 GB, Linux, rustc 1.99, clang and gcc 16); the
sample is 4,000 functions, **486,782 instructions, 10.6% of the game**, and the "whole game" figures are that times 9.47.

| | sample (4,000 functions) | whole game, scaled |
|---|---|---|
| generated source, C | 18.9 MB, 644,000 lines | about 180 MB, 6.1 million lines |
| generated source, Rust | 31.7 MB, 860,000 lines | about 300 MB, 8.1 million lines |
| **C, clang `-O0`, one thread** | 11.4 s | about 108 s |
| **C, clang `-O1`, one thread** | 31.8 s | **about 5 minutes** (about 40 s on 8 cores) |
| C, clang `-O2`, one thread | 37.2 s | about 6 minutes |
| C, gcc `-O1` (500 functions only) | 4.6 s against clang's 3.6 s | about the same, a little slower |
| C, memory | 144 MB for one 100-function file | small, and per file |
| **Rust, one crate, `opt-level` 0 or 1** | 78 s wall (24 threads), 126 s of CPU, **6.2 GB peak** | about 12 minutes and **about 59 GB**: not possible |
| Rust, a crate of 500 functions | 17.6 s wall, 20 s CPU | 80 such crates: about 27 minutes of CPU |

What it says:

- **Generate C.** The same code as C builds about four times faster than as Rust and needs a tenth of the memory per unit, and the Rust
  front end (type checking and borrow checking of millions of lines in one crate) is what costs, so turning optimisation off does not
  help: `opt-level` 0 took as long as 1. Rust would have to be cut into dozens of crates with no calls between them (calls through a
  table), which is possible but is more machinery for a slower build.
- A C compiler is not a new requirement: on Linux `cc` is nearly always there, and the Rust toolchain on Windows needs the Visual Studio
  build tools, which have `cl`. **Not tested:** MSVC's `cl` on this code (a Windows machine is needed), and clang-cl.
- **Numbers to treat as lower limits.** The placeholders and the simple memory model (no address checks, no byte-order tricks beyond
  a swap) are cheaper than what a real translator has to emit, and paired singles, exact rounding of single precision and
  reservations will add code. Allow for twice the figures: about ten minutes for the first build on one core, and under two on eight.
- **Not measured at all:** how fast the generated code *runs*. At `-O1` compile time is acceptable; whether the game reaches a frame
  rate depends on the runtime and the graphics, not on this.
- Build once on first start (or on a change of the game's files) and keep the result in the owner's cache folder: ten minutes once is
  acceptable, ten minutes every start is not.

## The executable's layout (for reference)

`.text` at `0x02000020` (7.9 MB compressed in the file, 18.4 MB unpacked), `.rodata` at `0x10000000`, `.data` at `0x10164640`, `.bss` at `0x1030a740`
(1.4 MB), the imports at `0xc0006040` on, and relocation tables for text, rodata and data. See `mh3u-tools sections`.
