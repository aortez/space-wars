//! The keys that combine into a position's hash.
//!
//! They come from a fixed sequence, so a position hashes to the same value on
//! every platform and in every run.

pub(crate) struct Keys {
    /// One key per colour, kind and square.
    pub(crate) pieces: [[[u64; 64]; 6]; 2],
    /// Present when Black is to move.
    pub(crate) black_to_move: u64,
    /// One key per combination of castling rights.
    pub(crate) castling: [u64; 16],
    /// One key per file of an en passant square.
    pub(crate) en_passant: [u64; 8],
}

/// SplitMix64: advances `state` and returns the next value of its sequence.
const fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

const fn generate() -> Keys {
    let mut state = 0x5eed_c4e5_5b0a_4d00;
    let mut keys = Keys {
        pieces: [[[0; 64]; 6]; 2],
        black_to_move: 0,
        castling: [0; 16],
        en_passant: [0; 8],
    };
    let mut color = 0;
    while color < 2 {
        let mut kind = 0;
        while kind < 6 {
            let mut square = 0;
            while square < 64 {
                keys.pieces[color][kind][square] = next(&mut state);
                square += 1;
            }
            kind += 1;
        }
        color += 1;
    }
    keys.black_to_move = next(&mut state);
    let mut rights = 0;
    while rights < 16 {
        keys.castling[rights] = next(&mut state);
        rights += 1;
    }
    let mut file = 0;
    while file < 8 {
        keys.en_passant[file] = next(&mut state);
        file += 1;
    }
    keys
}

pub(crate) static KEYS: Keys = generate();
