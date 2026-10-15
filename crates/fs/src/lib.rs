//! **Layer:** Infrastructure.
//!
//! **Responsibility:** file-system access implementing application ports, including loading rule
//! files (TOML) through the port the rules crate owns.
//!
//! **Permitted workspace dependencies:** `bleedthrough-app`, `bleedthrough-rules`, `bleedthrough-packs`.

mod rules_dir;
mod toml_rules;

pub use rules_dir::{
    CheckStatus, LoadedRules, RETIRED_FILE, TomlRuleDir, check_rules, content_hash, load_rules,
};
