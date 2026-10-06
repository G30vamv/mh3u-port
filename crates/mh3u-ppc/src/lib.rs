//! The Wii U's processor: decoding PowerPC instructions (the "Espresso" cores, a 32-bit PowerPC 750 with paired singles).
//!
//! Every instruction is one big-endian 32-bit word. [`decode`] says which instruction a word is and what kind of thing it does,
//! and nothing more; it does not run or translate anything. It exists to count what the game's code needs (see `docs/spikes.md`) and is
//! the base the translator would be built on. The instruction set is as the PowerPC architecture book and the Gekko/Broadway manuals give
//! it; where a word is none of those, [`decode`] says so rather than guessing.

/// What kind of thing an instruction does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// add, subtract, multiply, divide, negate, extend, count leading zeros.
    Integer,
    /// compare integers.
    Compare,
    /// and, or, xor, shifts and the like.
    Logical,
    /// rotate and mask.
    Rotate,
    /// branches, calls and returns.
    Branch,
    /// condition register logic and moves.
    CondReg,
    /// integer loads (bytes, halves, words, multiples, strings, reserved).
    Load,
    /// integer stores.
    Store,
    /// floating point loads and stores.
    FloatMem,
    /// floating point arithmetic and moves.
    Float,
    /// the paired-single instructions (two singles in one register), and their loads and stores.
    Paired,
    /// moves to and from special registers, the time base, the machine state.
    Special,
    /// cache control.
    Cache,
    /// synchronisation and the like.
    Sync,
    /// system call, trap, return from interrupt.
    System,
    /// instructions the architecture has for 64-bit processors only.
    Only64,
}

/// One decoded instruction: its name, kind and, for the few the translator treats specially, what is notable about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    pub name: &'static str,
    pub class: Class,
}

const fn d(name: &'static str, class: Class) -> Option<Decoded> {
    Some(Decoded { name, class })
}

use Class::*;

/// The bits of `word` from `from` to `to` counted the way the architecture books count them (bit 0 is the most significant).
fn bits(word: u32, from: u32, to: u32) -> u32 {
    (word >> (31 - to)) & ((1u32 << (to - from + 1)) - 1)
}

/// Decode one instruction word, or `None` if it is none of the instructions of this processor.
pub fn decode(word: u32) -> Option<Decoded> {
    let opcode = word >> 26;
    match opcode {
        3 => d("twi", System),
        4 => paired(word),
        7 => d("mulli", Integer),
        8 => d("subfic", Integer),
        10 => d("cmpli", Compare),
        11 => d("cmpi", Compare),
        12 => d("addic", Integer),
        13 => d("addic.", Integer),
        14 => d("addi", Integer),
        15 => d("addis", Integer),
        16 => d("bc", Branch),
        17 => d("sc", System),
        18 => d("b", Branch),
        19 => group19(word),
        20 => d("rlwimi", Rotate),
        21 => d("rlwinm", Rotate),
        23 => d("rlwnm", Rotate),
        24 => d("ori", Logical),
        25 => d("oris", Logical),
        26 => d("xori", Logical),
        27 => d("xoris", Logical),
        28 => d("andi.", Logical),
        29 => d("andis.", Logical),
        30 => d("rldic-family", Only64),
        31 => group31(word),
        32 => d("lwz", Load),
        33 => d("lwzu", Load),
        34 => d("lbz", Load),
        35 => d("lbzu", Load),
        36 => d("stw", Store),
        37 => d("stwu", Store),
        38 => d("stb", Store),
        39 => d("stbu", Store),
        40 => d("lhz", Load),
        41 => d("lhzu", Load),
        42 => d("lha", Load),
        43 => d("lhau", Load),
        44 => d("sth", Store),
        45 => d("sthu", Store),
        46 => d("lmw", Load),
        47 => d("stmw", Store),
        48 => d("lfs", FloatMem),
        49 => d("lfsu", FloatMem),
        50 => d("lfd", FloatMem),
        51 => d("lfdu", FloatMem),
        52 => d("stfs", FloatMem),
        53 => d("stfsu", FloatMem),
        54 => d("stfd", FloatMem),
        55 => d("stfdu", FloatMem),
        56 => d("psq_l", Paired),
        57 => d("psq_lu", Paired),
        58 => d("ld-family", Only64),
        59 => single_float(word),
        60 => d("psq_st", Paired),
        61 => d("psq_stu", Paired),
        62 => d("std-family", Only64),
        63 => double_float(word),
        _ => None,
    }
}

/// Opcode 19: branches to the link and count registers and condition register logic.
fn group19(word: u32) -> Option<Decoded> {
    match bits(word, 21, 30) {
        0 => d("mcrf", CondReg),
        16 => d("bclr", Branch),
        33 => d("crnor", CondReg),
        50 => d("rfi", System),
        129 => d("crandc", CondReg),
        150 => d("isync", Sync),
        193 => d("crxor", CondReg),
        225 => d("crnand", CondReg),
        257 => d("crand", CondReg),
        289 => d("creqv", CondReg),
        417 => d("crorc", CondReg),
        449 => d("cror", CondReg),
        528 => d("bcctr", Branch),
        _ => None,
    }
}

/// Opcode 31: integer arithmetic, logic, indexed loads and stores, special registers, cache and sync.
fn group31(word: u32) -> Option<Decoded> {
    let ext = bits(word, 21, 30);
    // the arithmetic forms have an overflow-enable bit (bit 21), which is not part of which instruction it is
    let arith = ext & 0x1ff;
    match arith {
        8 => return d("subfc", Integer),
        10 => return d("addc", Integer),
        136 => return d("subfe", Integer),
        138 => return d("adde", Integer),
        200 => return d("subfze", Integer),
        202 => return d("addze", Integer),
        232 => return d("subfme", Integer),
        234 => return d("addme", Integer),
        235 => return d("mullw", Integer),
        266 => return d("add", Integer),
        459 => return d("divwu", Integer),
        491 => return d("divw", Integer),
        40 => return d("subf", Integer),
        104 => return d("neg", Integer),
        11 => return d("mulhwu", Integer),
        75 => return d("mulhw", Integer),
        _ => {}
    }
    match ext {
        0 => d("cmp", Compare),
        4 => d("tw", System),
        19 => d("mfcr", CondReg),
        20 => d("lwarx", Load),
        23 => d("lwzx", Load),
        24 => d("slw", Logical),
        26 => d("cntlzw", Integer),
        28 => d("and", Logical),
        32 => d("cmpl", Compare),
        54 => d("dcbst", Cache),
        55 => d("lwzux", Load),
        60 => d("andc", Logical),
        83 => d("mfmsr", Special),
        86 => d("dcbf", Cache),
        87 => d("lbzx", Load),
        119 => d("lbzux", Load),
        124 => d("nor", Logical),
        144 => d("mtcrf", CondReg),
        146 => d("mtmsr", Special),
        150 => d("stwcx.", Store),
        151 => d("stwx", Store),
        183 => d("stwux", Store),
        210 => d("mtsr", Special),
        215 => d("stbx", Store),
        242 => d("mtsrin", Special),
        246 => d("dcbtst", Cache),
        247 => d("stbux", Store),
        278 => d("dcbt", Cache),
        279 => d("lhzx", Load),
        284 => d("eqv", Logical),
        306 => d("tlbie", System),
        310 => d("eciwx", Load),
        311 => d("lhzux", Load),
        316 => d("xor", Logical),
        339 => d("mfspr", Special),
        343 => d("lhax", Load),
        370 => d("tlbia", System),
        371 => d("mftb", Special),
        375 => d("lhaux", Load),
        407 => d("sthx", Store),
        412 => d("orc", Logical),
        438 => d("ecowx", Store),
        439 => d("sthux", Store),
        444 => d("or", Logical),
        467 => d("mtspr", Special),
        470 => d("dcbi", Cache),
        476 => d("nand", Logical),
        512 => d("mcrxr", CondReg),
        533 => d("lswx", Load),
        534 => d("lwbrx", Load),
        535 => d("lfsx", FloatMem),
        536 => d("srw", Logical),
        566 => d("tlbsync", System),
        567 => d("lfsux", FloatMem),
        595 => d("mfsr", Special),
        597 => d("lswi", Load),
        598 => d("sync", Sync),
        599 => d("lfdx", FloatMem),
        631 => d("lfdux", FloatMem),
        659 => d("mfsrin", Special),
        661 => d("stswx", Store),
        662 => d("stwbrx", Store),
        663 => d("stfsx", FloatMem),
        695 => d("stfsux", FloatMem),
        725 => d("stswi", Store),
        727 => d("stfdx", FloatMem),
        759 => d("stfdux", FloatMem),
        790 => d("lhbrx", Load),
        792 => d("sraw", Logical),
        824 => d("srawi", Logical),
        854 => d("eieio", Sync),
        918 => d("sthbrx", Store),
        922 => d("extsh", Integer),
        954 => d("extsb", Integer),
        982 => d("icbi", Cache),
        983 => d("stfiwx", FloatMem),
        1014 => d("dcbz", Cache),
        // the 64-bit processors' own: mulhd, ldx, sld, ... (not on this one)
        9 | 21 | 27 | 53 | 58 | 73 | 84 | 149 | 181 | 214 => d("64-bit-only", Only64),
        _ => None,
    }
}

/// Opcode 4: the paired singles (two single-precision values in one floating point register), a Gekko/Broadway/Espresso extension.
fn paired(word: u32) -> Option<Decoded> {
    // the arithmetic forms use bits 26-30 (five bits); the others use bits 21-30
    match bits(word, 26, 30) {
        10 => return d("ps_sum0", Paired),
        11 => return d("ps_sum1", Paired),
        12 => return d("ps_muls0", Paired),
        13 => return d("ps_muls1", Paired),
        14 => return d("ps_madds0", Paired),
        15 => return d("ps_madds1", Paired),
        18 => return d("ps_div", Paired),
        20 => return d("ps_sub", Paired),
        21 => return d("ps_add", Paired),
        23 => return d("ps_sel", Paired),
        24 => return d("ps_res", Paired),
        25 => return d("ps_mul", Paired),
        26 => return d("ps_rsqrte", Paired),
        28 => return d("ps_msub", Paired),
        29 => return d("ps_madd", Paired),
        30 => return d("ps_nmsub", Paired),
        31 => return d("ps_nmadd", Paired),
        _ => {}
    }
    match bits(word, 21, 30) {
        0 => d("ps_cmpu0", Paired),
        32 => d("ps_cmpo0", Paired),
        40 => d("ps_neg", Paired),
        64 => d("ps_cmpu1", Paired),
        72 => d("ps_mr", Paired),
        96 => d("ps_cmpo1", Paired),
        136 => d("ps_nabs", Paired),
        264 => d("ps_abs", Paired),
        528 => d("ps_merge00", Paired),
        560 => d("ps_merge01", Paired),
        592 => d("ps_merge10", Paired),
        624 => d("ps_merge11", Paired),
        1014 => d("dcbz_l", Cache),
        _ => match bits(word, 25, 30) {
            6 => d("psq_lx", Paired),
            7 => d("psq_stx", Paired),
            38 => d("psq_lux", Paired),
            39 => d("psq_stux", Paired),
            _ => None,
        },
    }
}

/// Opcode 59: single-precision arithmetic.
fn single_float(word: u32) -> Option<Decoded> {
    match bits(word, 26, 30) {
        18 => d("fdivs", Float),
        20 => d("fsubs", Float),
        21 => d("fadds", Float),
        22 => d("fsqrts", Float),
        24 => d("fres", Float),
        25 => d("fmuls", Float),
        28 => d("fmsubs", Float),
        29 => d("fmadds", Float),
        30 => d("fnmsubs", Float),
        31 => d("fnmadds", Float),
        _ => None,
    }
}

/// Opcode 63: double-precision arithmetic, comparison, conversion and the floating point status register.
fn double_float(word: u32) -> Option<Decoded> {
    match bits(word, 26, 30) {
        18 => return d("fdiv", Float),
        20 => return d("fsub", Float),
        21 => return d("fadd", Float),
        22 => return d("fsqrt", Float),
        23 => return d("fsel", Float),
        25 => return d("fmul", Float),
        26 => return d("frsqrte", Float),
        28 => return d("fmsub", Float),
        29 => return d("fmadd", Float),
        30 => return d("fnmsub", Float),
        31 => return d("fnmadd", Float),
        _ => {}
    }
    match bits(word, 21, 30) {
        0 => d("fcmpu", Float),
        12 => d("frsp", Float),
        14 => d("fctiw", Float),
        15 => d("fctiwz", Float),
        32 => d("fcmpo", Float),
        38 => d("mtfsb1", Special),
        40 => d("fneg", Float),
        64 => d("mcrfs", Special),
        70 => d("mtfsb0", Special),
        72 => d("fmr", Float),
        134 => d("mtfsfi", Special),
        136 => d("fnabs", Float),
        264 => d("fabs", Float),
        583 => d("mffs", Special),
        711 => d("mtfsf", Special),
        _ => None,
    }
}

/// Which special register a `mfspr` or `mtspr` word names (the halves of the number are swapped in the encoding).
pub fn spr(word: u32) -> u32 {
    let field = bits(word, 11, 20);
    ((field & 0x1f) << 5) | (field >> 5)
}

/// A name for the special registers the game's code uses, `None` for ones with no usual name here.
pub fn spr_name(number: u32) -> Option<&'static str> {
    Some(match number {
        1 => "XER",
        8 => "LR",
        9 => "CTR",
        18 => "DSISR",
        19 => "DAR",
        22 => "DEC",
        26 => "SRR0",
        27 => "SRR1",
        268 => "TBL",
        269 => "TBU",
        272..=275 => "SPRG",
        528..=543 => "BAT",
        912..=919 => "GQR",
        920 => "HID2",
        921 => "WPAR",
        922 => "DMA_U",
        923 => "DMA_L",
        936..=939 => "UMMCR/UPMC",
        952..=959 => "MMCR/PMC/SIA",
        1008 => "HID0",
        1009 => "HID1",
        1010 => "IABR",
        1013 => "DABR",
        1017 => "L2CR",
        1019 => "ICTC",
        1020 => "THRM",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(word: u32) -> &'static str {
        decode(word).map_or("?", |d| d.name)
    }

    #[test]
    fn well_known_words_decode_to_the_instructions_they_are() {
        assert_eq!(name(0x4e80_0020), "bclr", "blr");
        assert_eq!(name(0x7c08_02a6), "mfspr", "mflr r0");
        assert_eq!(name(0x7c08_03a6), "mtspr", "mtlr r0");
        assert_eq!(name(0x9421_ffe0), "stwu", "stwu r1,-32(r1)");
        assert_eq!(name(0x3860_0001), "addi", "li r3,1");
        assert_eq!(name(0x6000_0000), "ori", "nop");
        assert_eq!(name(0x8003_0000), "lwz");
        assert_eq!(name(0xc003_0000), "lfs");
        assert_eq!(name(0x4800_0001), "b", "bl");
        assert_eq!(name(0x4e80_0420), "bcctr", "bctr");
        assert_eq!(name(0x7c00_04ac), "sync");
        assert_eq!(name(0x7c00_0378), "or", "mr r0,r0");
        assert_eq!(name(0x5460_063e), "rlwinm");
        assert_eq!(name(0x7c00_0214), "add");
        assert_eq!(name(0x7c00_0a14), "add", "addo is the same instruction with the overflow bit");
        assert_eq!(name(0xec00_182a), "fadds");
        assert_eq!(name(0xfc00_0050), "fneg");
        assert_eq!(name(0x1000_0420), "ps_merge00", "paired singles");
        assert_eq!(name(0x1000_002a), "ps_add", "paired singles, the five-bit forms");
        assert_eq!(name(0xe000_0000), "psq_l");
    }

    #[test]
    fn words_that_are_no_instruction_are_not_guessed() {
        assert_eq!(decode(0x0000_0000), None, "zero is not an instruction");
        assert_eq!(decode(0x0400_0000), None, "opcode 1");
        assert_eq!(decode(0x7c00_0002), None, "opcode 31 with an extended opcode nobody has");
    }

    #[test]
    fn the_special_register_number_has_its_halves_swapped_back() {
        assert_eq!(spr(0x7c08_02a6), 8, "LR");
        assert_eq!(spr(0x7c09_03a6), 9, "CTR");
        assert_eq!(spr_name(8), Some("LR"));
        assert_eq!(spr_name(912), Some("GQR"));
        // GQR0 is spr 912, which is written with its halves swapped
        assert_eq!(spr(0x7c10_e3a6), 912);
        assert_eq!(spr_name(spr(0x7c10_e3a6)), Some("GQR"));
    }
}
