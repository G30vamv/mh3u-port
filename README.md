# mh3u-port

Groundwork for a native PC port of Monster Hunter 3 Ultimate (Wii U) for Linux and Windows. **Private for now.** Nothing here runs
the game yet; read [`docs/pc-port.md`](docs/pc-port.md) for what the project is meant to be, and [`docs/spikes.md`](docs/spikes.md)
for what has been measured so far.

**You must own the game.** This repository contains no game code and no game data, and the port will not run without your own dump of
your own copy of the Wii U game (base game `0005000010118300` and its update `0005000e10118300`). See the first section of
`docs/pc-port.md`.

## What is here

| Crate | What it does |
|---|---|
| `mh3u-formats` | the game's executable (`.rpx`: sections, imports), archives (`.arc`) and text (`.gmd`), with bounded decompression and tests on damaged files |
| `mh3u-dump` | finds the owner's dump and checks that it is this game |
| `mh3u-ppc` | decodes the Wii U's PowerPC instructions (spike 2) |
| `mh3u-latte` | reads the Wii U GPU's shader programs and counts what is in them (spike 3) |
| `mh3u-cemu` | reads the memory of a running Cemu (Linux), to compare the port with the emulator |
| `mh3u-tools` | explorers: `dump`, `sections`, `imports`, `coverage`, `shaders` (spike 3), `scan-shaders` (spike 3, needs Cemu), `program-census`, `section`, `codegen-bench` (spike 4), `arcls`, `arcx`, `gmd`, `arcsearch` |

```
cargo run -p mh3u-tools -- dump "/path/to/your/games folder"
cargo run -p mh3u-tools -- sections "/path/to/your/games folder"
cargo run -p mh3u-tools -- imports "/path/to/your/games folder"
```

The first four crates started as copies of the readers in the [companion app](https://github.com/glinesbdev/mh3u-companion) (public
domain, same author); from here on they change on their own. Run `scripts/check.sh` before every commit; there is no CI.

## Licence

Public domain (`LICENSE`), for the code here. It says nothing about the game, which is Capcom's. The repository is private; decide
before it goes public whether this is still the licence.
