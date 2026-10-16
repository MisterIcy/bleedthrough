//! The severity model: CVSS 3.1 base metrics, the base-score equations in exact integer
//! arithmetic, the vector string and the level mapping.
//!
//! The value is the rule author's base-metric estimate for a class of weakness, not an official
//! CVSS assessment of a specific vulnerability. Equations, metric weights and the round-up rule
//! are those of the CVSS v3.1 specification (<https://www.first.org/cvss/v3.1/specification-document>,
//! section 7 and Appendix A).

use std::fmt;

use crate::SeverityLevel;

/// The version of the severity scheme (metrics, formula and level thresholds), recorded in
/// reports. Any change to the formula or the thresholds changes it.
pub const SEVERITY_SCHEME: u32 = 1;

macro_rules! metric {
    ($(#[$m:meta])* $name:ident, $key:literal, { $($variant:ident => $word:literal, $letter:literal;)+ }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $(
                #[doc = $word]
                $variant,
            )+
        }

        impl $name {
            /// The metric's key in a vector string, such as `AV`.
            pub const KEY: &'static str = $key;

            /// The lowercase word used in rule files.
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $word,)+ }
            }

            /// The one-letter value used in vector strings.
            pub fn letter(&self) -> char {
                match self { $(Self::$variant => $letter,)+ }
            }

            /// Parses the lowercase word; any other text is `None`.
            pub fn parse(text: &str) -> Option<Self> {
                match text { $($word => Some(Self::$variant),)+ _ => None }
            }

            /// The accepted words, for error messages.
            pub fn words() -> &'static [&'static str] {
                &[$($word),+]
            }
        }
    };
}

metric!(
    /// Term: Severity metrics
    ///
    /// CVSS 3.1 attack vector: how remote the attacker can be.
    AttackVector, "AV", {
        Network => "network", 'N';
        Adjacent => "adjacent", 'A';
        Local => "local", 'L';
        Physical => "physical", 'P';
    }
);
metric!(
    /// Term: Severity metrics
    ///
    /// CVSS 3.1 attack complexity.
    AttackComplexity, "AC", {
        Low => "low", 'L';
        High => "high", 'H';
    }
);
metric!(
    /// Term: Severity metrics
    ///
    /// CVSS 3.1 privileges required.
    PrivilegesRequired, "PR", {
        None => "none", 'N';
        Low => "low", 'L';
        High => "high", 'H';
    }
);
metric!(
    /// Term: Severity metrics
    ///
    /// CVSS 3.1 user interaction.
    UserInteraction, "UI", {
        None => "none", 'N';
        Required => "required", 'R';
    }
);
metric!(
    /// Term: Severity metrics
    ///
    /// CVSS 3.1 scope: whether the impact crosses a security boundary.
    Scope, "S", {
        Unchanged => "unchanged", 'U';
        Changed => "changed", 'C';
    }
);
metric!(
    /// Term: Severity metrics
    ///
    /// CVSS 3.1 impact on confidentiality, integrity or availability.
    Impact, "C", {
        None => "none", 'N';
        Low => "low", 'L';
        High => "high", 'H';
    }
);

/// Term: Severity metrics
///
/// The eight CVSS 3.1 base metrics a rule declares. All are required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs)]
pub struct Metrics {
    /// Attack vector.
    pub attack_vector: AttackVector,
    /// Attack complexity.
    pub attack_complexity: AttackComplexity,
    /// Privileges required.
    pub privileges_required: PrivilegesRequired,
    /// User interaction.
    pub user_interaction: UserInteraction,
    /// Scope.
    pub scope: Scope,
    /// Confidentiality impact.
    pub confidentiality: Impact,
    /// Integrity impact.
    pub integrity: Impact,
    /// Availability impact.
    pub availability: Impact,
}

/// Fixed-point scale (10^18) for intermediate values.
const ONE: i128 = 1_000_000_000_000_000_000;

impl Metrics {
    /// The CVSS 3.1 vector string, such as `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H`.
    pub fn vector(&self) -> String {
        format!(
            "CVSS:3.1/AV:{}/AC:{}/PR:{}/UI:{}/S:{}/C:{}/I:{}/A:{}",
            self.attack_vector.letter(),
            self.attack_complexity.letter(),
            self.privileges_required.letter(),
            self.user_interaction.letter(),
            self.scope.letter(),
            self.confidentiality.letter(),
            self.integrity.letter(),
            self.availability.letter(),
        )
    }

    /// The base score in tenths (98 means 9.8), by the CVSS 3.1 equations in exact arithmetic.
    pub fn score_tenths(&self) -> u8 {
        let changed = self.scope == Scope::Changed;
        // Metric weights in thousandths, as in the specification's table of metric values.
        let av: i128 = match self.attack_vector {
            AttackVector::Network => 850,
            AttackVector::Adjacent => 620,
            AttackVector::Local => 550,
            AttackVector::Physical => 200,
        };
        let ac: i128 = match self.attack_complexity {
            AttackComplexity::Low => 770,
            AttackComplexity::High => 440,
        };
        let pr: i128 = match (self.privileges_required, changed) {
            (PrivilegesRequired::None, _) => 850,
            (PrivilegesRequired::Low, false) => 620,
            (PrivilegesRequired::Low, true) => 680,
            (PrivilegesRequired::High, false) => 270,
            (PrivilegesRequired::High, true) => 500,
        };
        let ui: i128 = match self.user_interaction {
            UserInteraction::None => 850,
            UserInteraction::Required => 620,
        };
        let cia = |i: Impact| -> i128 {
            match i {
                Impact::High => 560,
                Impact::Low => 220,
                Impact::None => 0,
            }
        };

        // ISS = 1 - (1-C)(1-I)(1-A); the product of thousandths is exact at scale 10^9.
        let product = (1000 - cia(self.confidentiality))
            * (1000 - cia(self.integrity))
            * (1000 - cia(self.availability));
        let iss = ONE - product * (ONE / 1_000_000_000);

        let impact = if changed {
            // 7.52 x (ISS - 0.029) - 3.25 x (ISS - 0.02)^15
            let base = iss - ONE / 1000 * 20;
            752 * (iss - ONE / 1000 * 29) / 100 - 325 * pow15(base) / 100
        } else {
            642 * iss / 100
        };
        if impact <= 0 {
            return 0;
        }
        // 8.22 x AV x AC x PR x UI; the product of four thousandths is exact at scale 10^12.
        let exploitability = 822 * (av * ac * pr * ui) * (ONE / 100_000_000_000_000);
        let sum = impact + exploitability;
        let sum = if changed { sum * 108 / 100 } else { sum };
        roundup_tenths(sum.min(10 * ONE))
    }

    /// The severity level the score maps to.
    pub fn level(&self) -> SeverityLevel {
        level_for_tenths(self.score_tenths())
    }
}

/// `x^15` for a fixed-point `x` between 0 and 1.
fn pow15(x: i128) -> i128 {
    let mul = |a: i128, b: i128| a * b / ONE;
    let x2 = mul(x, x);
    let x4 = mul(x2, x2);
    let x8 = mul(x4, x4);
    // 15 = 8 + 4 + 2 + 1
    mul(mul(x8, x4), mul(x2, x))
}

/// The specification's `Roundup` on integers (Appendix A): round the input to five decimals, then
/// take the smallest one-decimal value that is not below it.
fn roundup_tenths(value: i128) -> u8 {
    let int_input = (value * 100_000 + ONE / 2) / ONE;
    let tenths = if int_input % 10_000 == 0 {
        int_input / 10_000
    } else {
        int_input / 10_000 + 1
    };
    u8::try_from(tenths).unwrap_or(100)
}

/// Maps a score in tenths to its level: 0.0 info, 0.1 to 3.9 low, 4.0 to 6.9 medium, 7.0 to 8.9
/// high, 9.0 and above critical.
pub fn level_for_tenths(tenths: u8) -> SeverityLevel {
    match tenths {
        0 => SeverityLevel::Info,
        1..=39 => SeverityLevel::Low,
        40..=69 => SeverityLevel::Medium,
        70..=89 => SeverityLevel::High,
        _ => SeverityLevel::Critical,
    }
}

impl fmt::Display for Metrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.vector())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `CVSS:3.1/AV:N/...` into metrics, for the reference table.
    fn parse(vector: &str) -> Metrics {
        let mut parts = vector.split('/');
        assert_eq!(parts.next(), Some("CVSS:3.1"));
        let v: Vec<char> = parts.map(|p| p.chars().last().unwrap()).collect();
        let imp = |c| match c {
            'H' => Impact::High,
            'L' => Impact::Low,
            _ => Impact::None,
        };
        Metrics {
            attack_vector: match v[0] {
                'N' => AttackVector::Network,
                'A' => AttackVector::Adjacent,
                'L' => AttackVector::Local,
                _ => AttackVector::Physical,
            },
            attack_complexity: if v[1] == 'L' {
                AttackComplexity::Low
            } else {
                AttackComplexity::High
            },
            privileges_required: match v[2] {
                'N' => PrivilegesRequired::None,
                'L' => PrivilegesRequired::Low,
                _ => PrivilegesRequired::High,
            },
            user_interaction: if v[3] == 'N' {
                UserInteraction::None
            } else {
                UserInteraction::Required
            },
            scope: if v[4] == 'U' {
                Scope::Unchanged
            } else {
                Scope::Changed
            },
            confidentiality: imp(v[5]),
            integrity: imp(v[6]),
            availability: imp(v[7]),
        }
    }

    /// Base scores published as CVSS 3.1 examples at <https://www.first.org/cvss/examples>
    /// (fetched 2026-10-15), including scope-changed vectors. The zero-impact vector is defined by
    /// the specification (`Impact <= 0` gives 0).
    const REFERENCE: [(&str, u8); 24] = [
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 98),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H", 100),
        ("CVSS:3.1/AV:L/AC:H/PR:L/UI:N/S:U/C:H/I:H/A:H", 70),
        ("CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:H/A:H", 81),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N", 75),
        ("CVSS:3.1/AV:L/AC:L/PR:N/UI:R/S:U/C:L/I:N/A:N", 33),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:C/C:L/I:L/A:N", 61),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:L/I:L/A:N", 72),
        ("CVSS:3.1/AV:L/AC:L/PR:H/UI:N/S:C/C:H/I:N/A:N", 60),
        ("CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H", 93),
        ("CVSS:3.1/AV:P/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H", 68),
        ("CVSS:3.1/AV:A/AC:L/PR:N/UI:N/S:C/C:H/I:N/A:H", 93),
        ("CVSS:3.1/AV:L/AC:L/PR:H/UI:N/S:C/C:H/I:H/A:H", 82),
        ("CVSS:3.1/AV:L/AC:L/PR:H/UI:N/S:U/C:N/I:H/A:H", 60),
        ("CVSS:3.1/AV:P/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H", 76),
        ("CVSS:3.1/AV:N/AC:L/PR:L/UI:R/S:C/C:L/I:L/A:N", 54),
        ("CVSS:3.1/AV:N/AC:L/PR:L/UI:R/S:C/C:H/I:H/A:H", 90),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:U/C:N/I:L/A:N", 43),
        ("CVSS:3.1/AV:N/AC:L/PR:H/UI:N/S:U/C:H/I:N/A:N", 49),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:N/I:L/A:N", 58),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:L/A:L", 73),
        ("CVSS:3.1/AV:P/AC:H/PR:N/UI:N/S:U/C:H/I:H/A:H", 64),
        ("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:N/I:N/A:N", 0),
        ("CVSS:3.1/AV:L/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N", 0),
    ];

    /// spec: severity-scoring/A fixed formula derives the score
    #[test]
    fn reference_vectors_round_trip_and_score() {
        for (vector, tenths) in REFERENCE {
            let m = parse(vector);
            assert_eq!(m.vector(), vector);
            assert_eq!(m.score_tenths(), tenths, "{vector}");
        }
    }

    #[test]
    fn rendered_vector_for_the_reference_metrics() {
        let m = parse("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H");
        assert_eq!(
            m.to_string(),
            "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"
        );
        assert_eq!(m.level(), SeverityLevel::Critical);
    }

    /// spec: severity-scoring/Level is derived from the score
    #[test]
    fn level_boundaries() {
        use SeverityLevel::*;
        let cases = [
            (0, Info),
            (1, Low),
            (39, Low),
            (40, Medium),
            (69, Medium),
            (70, High),
            (89, High),
            (90, Critical),
            (100, Critical),
        ];
        for (tenths, level) in cases {
            assert_eq!(level_for_tenths(tenths), level, "{tenths}");
        }
        assert_eq!(SEVERITY_SCHEME, 1);
    }

    #[test]
    fn words_and_letters_parse() {
        assert_eq!(AttackVector::parse("network"), Some(AttackVector::Network));
        assert_eq!(AttackVector::parse("N"), None);
        assert_eq!(Scope::Changed.letter(), 'C');
        assert_eq!(Impact::words(), ["none", "low", "high"]);
    }

    #[test]
    fn roundup_rounds_to_five_decimals_before_taking_the_next_tenth() {
        let scaled = |micro: i128| micro * (ONE / 1_000_000);
        // exactly on a tenth stays
        assert_eq!(roundup_tenths(scaled(4_100_000)), 41);
        // below half of the fifth decimal rounds down to the tenth
        assert_eq!(roundup_tenths(scaled(4_100_004) + ONE / 10_000_000), 41);
        // from half of the fifth decimal upwards rounds the input up first, so the tenth moves on
        assert_eq!(roundup_tenths(scaled(4_100_005)), 42);
        assert_eq!(roundup_tenths(scaled(4_100_009) + ONE / 10_000_000 * 6), 42);
        assert_eq!(roundup_tenths(scaled(4_100_010)), 42);
        // a value that rounds up to the tenth is still that tenth
        assert_eq!(roundup_tenths(scaled(4_099_996)), 41);
        assert_eq!(roundup_tenths(0), 0);
    }
}
