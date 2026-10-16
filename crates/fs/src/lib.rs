//! **Layer:** Infrastructure.
//!
//! **Responsibility:** file-system access implementing application ports, including loading rule
//! files (TOML) through the port the rules crate owns.
//!
//! **Permitted workspace dependencies:** `bleedthrough-app`, `bleedthrough-rules`, `bleedthrough-packs`, `bleedthrough-symbols`, `bleedthrough-builtins`.

mod builtin_data;
mod composer;
mod rules_dir;
mod toml_rules;

pub use builtin_data::EmbeddedBuiltinData;
pub use composer::{
    AutoloadMap, AutoloadRules, ComposerDiagnostic, ComposerLocator, ComposerProject, DirEntry, Fs,
    LockedPackage, MemFs, OsFs, PrefixRule, normalise, read_rules,
};
pub use rules_dir::{
    CheckStatus, LoadedRules, RETIRED_FILE, TomlRuleDir, check_rules, content_hash, load_rules,
};
