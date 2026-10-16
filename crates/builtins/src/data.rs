//! The validated, indexed collection of records and its lookups.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

use crate::record::{ExtensionRecord, Kind, Record, normalise_name};
use crate::validate::check_record;
use crate::version::{PhpVersion, VersionMask};

/// Term: exempt
///
/// Why records could not be assembled into data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of the symbol and the problem"
)]
pub struct DataError {
    /// The symbol (or extension) concerned, as written in its record.
    pub symbol: String,
    /// What is wrong.
    pub problem: String,
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.symbol, self.problem)
    }
}

impl std::error::Error for DataError {}

/// Term: Built-in data
///
/// The result of looking a symbol up for one PHP version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Lookup<'a> {
    /// The record whose mask contains the version.
    Found(&'a Record),
    /// The symbol exists in other versions but not in this one.
    NotInVersion,
    /// The symbol exists in other versions, and its extension is not bundled in this one.
    Unbundled,
    /// No such symbol in any version.
    Unknown,
}

/// The canonical order of two records: kind, normalised name, first version, then exact name.
pub fn canonical_order(a: &Record, b: &Record) -> Ordering {
    a.key()
        .cmp(&b.key())
        .then_with(|| first_version(a.mask).cmp(&first_version(b.mask)))
        .then_with(|| a.mask.cmp(&b.mask))
        .then_with(|| a.name.cmp(&b.name))
}

fn first_version(mask: VersionMask) -> u32 {
    mask.bits().trailing_zeros()
}

/// Term: Built-in data
///
/// All built-in symbols for PHP 8.0 to 8.5: validated records in canonical order, with lookups
/// that depend only on the data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinData {
    extensions: Vec<ExtensionRecord>,
    records: Vec<Record>,
    by_key: BTreeMap<(Kind, String), Range<usize>>,
    by_owner: BTreeMap<String, Vec<usize>>,
}

impl BuiltinData {
    /// Validates `records`, puts them in canonical order and indexes them. Fails on an invalid
    /// field, on two records of one symbol whose masks overlap, and on a repeated extension.
    pub fn new(
        mut extensions: Vec<ExtensionRecord>,
        mut records: Vec<Record>,
    ) -> Result<Self, DataError> {
        for record in &records {
            check_record(record).map_err(|problem| DataError {
                symbol: record.name.clone(),
                problem,
            })?;
        }
        extensions.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(pair) = extensions.windows(2).find(|w| w[0].name == w[1].name) {
            return Err(DataError {
                symbol: pair[0].name.clone(),
                problem: "extension is listed twice".to_string(),
            });
        }
        records.sort_by(canonical_order);
        let mut by_key: BTreeMap<(Kind, String), Range<usize>> = BTreeMap::new();
        let mut by_owner: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, record) in records.iter().enumerate() {
            let range = by_key.entry(record.key()).or_insert(index..index);
            if let Some(previous) = records[range.clone()]
                .iter()
                .find(|r| r.mask.overlaps(record.mask))
            {
                return Err(DataError {
                    symbol: record.name.clone(),
                    problem: format!(
                        "version masks {} and {} overlap",
                        previous.mask, record.mask
                    ),
                });
            }
            range.end = index + 1;
            if let Some(owner) = record.owner() {
                by_owner
                    .entry(owner.to_ascii_lowercase())
                    .or_default()
                    .push(index);
            }
        }
        Ok(Self {
            extensions,
            records,
            by_key,
            by_owner,
        })
    }

    /// Data with no symbols.
    pub fn empty() -> Self {
        Self {
            extensions: Vec::new(),
            records: Vec::new(),
            by_key: BTreeMap::new(),
            by_owner: BTreeMap::new(),
        }
    }

    /// Every record in canonical order.
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// The bundling information of extensions, sorted by name.
    pub fn extensions(&self) -> &[ExtensionRecord] {
        &self.extensions
    }

    /// The extension record named `name`, if any.
    pub fn extension(&self, name: &str) -> Option<&ExtensionRecord> {
        self.extensions
            .binary_search_by(|e| e.name.as_str().cmp(name))
            .ok()
            .map(|i| &self.extensions[i])
    }

    /// Every record of one symbol, in version order, whatever the version.
    pub fn symbol(&self, kind: Kind, name: &str) -> &[Record] {
        self.by_key
            .get(&(kind, normalise_name(kind, name)))
            .map_or(&[], |range| &self.records[range.clone()])
    }

    /// Looks a symbol up for one PHP version.
    pub fn lookup(&self, kind: Kind, name: &str, version: PhpVersion) -> Lookup<'_> {
        let records = self.symbol(kind, name);
        if let Some(found) = records.iter().find(|r| r.mask.contains(version)) {
            return Lookup::Found(found);
        }
        let unbundled = records.first().is_some_and(|r| {
            self.extension(&r.extension)
                .is_some_and(|e| e.unbundled.contains(version))
        });
        match (records.is_empty(), unbundled) {
            (true, _) => Lookup::Unknown,
            (false, true) => Lookup::Unbundled,
            (false, false) => Lookup::NotInVersion,
        }
    }

    /// The members (constants, cases, properties, methods) of a class-like that exist in
    /// `version`, in canonical order.
    pub fn members<'a>(
        &'a self,
        class: &str,
        version: PhpVersion,
    ) -> impl Iterator<Item = &'a Record> {
        let class = class
            .strip_prefix('\\')
            .unwrap_or(class)
            .to_ascii_lowercase();
        self.by_owner
            .get(&class)
            .into_iter()
            .flatten()
            .map(|i| &self.records[*i])
            .filter(move |r| r.mask.contains(version))
    }
}
