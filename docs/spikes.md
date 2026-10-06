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

## 2. Instruction coverage (not done)

The code section (`.text`, at `0x02000020`) is 7,920,248 bytes: about 1.98 million instructions. The question is how many kinds
there are and how many are unusual.

## 3. One frame (not done)

The question is whether the graphics layer is reachable: take one known draw call from a Cemu capture and say what it needs.

## The executable's layout (for reference)

`.text` at `0x02000020` (7.9 MB, compressed in the file), `.rodata` at `0x10000000`, `.data` at `0x10164640`, `.bss` at `0x1030a740`
(1.4 MB), the imports at `0xc0006040` on, and relocation tables for text, rodata and data. See `mh3u-tools sections`.
