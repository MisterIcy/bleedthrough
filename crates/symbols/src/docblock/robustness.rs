//! Random and truncated docblock text must never panic or hang the parsers. The generator is a
//! small deterministic SplitMix64, like the one in the syntax conformance suite, so a failure
//! message carrying the seed reproduces the exact input.

use super::*;

struct Prng(u64);

impl Prng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next_u64() % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
    }
}

const SEEDS: &[&str] = &[
    "/**\n * @param array<string, list<int>> $x the map\n * @return array{id: int, name?: string}|null\n */",
    "/** @method static Foo create(array<string> $data, int ...$more) */",
    "/**\n * @template T of Foo\n * @param class-string<T> $c\n * @return callable(int, T=): void\n */",
    "/** @var int<0, max>|'a'|\"b\"|Suit::Hearts|(A&B) $x */",
    "/** @param Ünïcode\u{20ac}|array{'k é': string} $é */",
    "/** @psalm-return non-empty-array<array-key, iterable<int, \\App\\Foo>> */",
];

const INTERESTING: &[char] = &[
    '{', '}', '(', ')', '[', ']', '<', '>', ',', ':', '|', '&', '?', '=', '*', '$', '\'', '"',
    '\\', '-', '.', '@', '/', ' ', '\n', 'a', 'é', '€', '0', '9',
];

fn mutate(seed: &str, rng: &mut Prng) -> String {
    let mut chars: Vec<char> = seed.chars().collect();
    for _ in 0..=rng.below(4) {
        let at = rng.below(chars.len() + 1);
        match rng.below(3) {
            0 if at < chars.len() => {
                chars.remove(at);
            }
            1 => chars.insert(at, INTERESTING[rng.below(INTERESTING.len())]),
            _ if at < chars.len() => chars[at] = INTERESTING[rng.below(INTERESTING.len())],
            _ => {}
        }
    }
    chars.into_iter().collect()
}

fn exercise(text: &str) {
    let ctx = TypeContext::new();
    let _ = parse_docblock(text, 7, &ctx);
    let _ = parse_type(text, &ctx, 0);
    let inner = text.trim_start_matches("/**").trim_end_matches("*/");
    let _ = parse_type(inner, &ctx, 0);
}

#[test]
fn every_prefix_of_a_docblock_is_handled() {
    for seed in SEEDS {
        for (end, _) in seed.char_indices() {
            exercise(&seed[..end]);
        }
        exercise(seed);
    }
}

#[test]
fn seeded_mutations_are_handled() {
    for (index, seed) in SEEDS.iter().enumerate() {
        for round in 0..2000u64 {
            let state = 0xB1EE_D000 + (index as u64) * 100_000 + round;
            let mut rng = Prng(state);
            let text = mutate(seed, &mut rng);
            exercise(&text);
        }
    }
}

#[test]
fn random_text_is_handled() {
    let mut rng = Prng(42);
    for _ in 0..3000 {
        let len = rng.below(80);
        let text: String = (0..len)
            .map(|_| INTERESTING[rng.below(INTERESTING.len())])
            .collect();
        exercise(&text);
        exercise(&format!("/** @param {text} */"));
        exercise(&format!("/** @method {text} */"));
    }
}

#[test]
fn pathological_nesting_and_length_complete() {
    let ctx = TypeContext::new();
    let open = "array<".repeat(5000);
    let _ = parse_type(&open, &ctx, 0);
    let unions = vec!["int"; 20_000].join("|");
    assert!(parse_type(&unions, &ctx, 0).ty.is_some());
    let shape = format!("array{{{}}}", "a: int, ".repeat(5000));
    let _ = parse_type(&shape, &ctx, 0);
    let _ = parse_docblock(
        &format!("/**{}*/", "\n * @param int $x".repeat(5000)),
        0,
        &ctx,
    );
}
