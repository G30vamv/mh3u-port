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

## 4. Does the generated code compile in a reasonable time? (not done, and needed before the first translator)

Translate a few thousand real functions to Rust and to C, compile each set, and time it; scale the result to 4.6 million instructions.
If Rust cannot do it in minutes on an ordinary machine, the plan changes (C and a bundled compiler, or an interpreter/JIT for the cold
code), and `docs/pc-port.md` says so.

## The executable's layout (for reference)

`.text` at `0x02000020` (7.9 MB compressed in the file, 18.4 MB unpacked), `.rodata` at `0x10000000`, `.data` at `0x10164640`, `.bss` at `0x1030a740`
(1.4 MB), the imports at `0xc0006040` on, and relocation tables for text, rodata and data. See `mh3u-tools sections`.
