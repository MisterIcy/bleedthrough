//! Deterministic generator of hostile strings for the escape round-trip property tests.

const ALPHABET: [&str; 16] = [
    "a",
    "Z",
    "0",
    " ",
    "\t",
    "\n",
    "\r",
    "\\",
    "%",
    ":",
    ",",
    "::",
    "é",
    "日",
    "\u{1F600}",
    "\u{0}",
];

/// A fixed pseudo-random (xorshift) sample of strings built from troublesome characters.
pub fn hostile_strings() -> Vec<String> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..500)
        .map(|_| {
            let len = (next() % 12) as usize;
            (0..len)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
                .collect()
        })
        .collect()
}
