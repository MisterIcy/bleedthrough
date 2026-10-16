//! The canonical text format: one record per line, tab-separated fields in a fixed order.
//!
//! ```text
//! bleedthrough-builtins 1
//! extension  <name>  <unbundled mask>
//! <kind>  <name>  <mask>  <reflected mask>  <extension>  <sapis>  <guards>  <deprecation>  <details...>
//! ```
//!
//! Lists use `,` (SAPIs, parents, interfaces, flags) or `;` (guards, parameters); a parameter is
//! `name@flags@type@doctype@default`. A deprecation is `since:message`, empty when there is none.
//! Escaping is `\\`, `\t`, `\n`, `\r` and `\xHH` (control characters and the delimiters of the
//! field's context), and nothing else. The reader accepts only text that the writer would produce.

use std::fmt;

use crate::data::{BuiltinData, canonical_order};
use crate::record::{
    CaseDetail, ClassDetail, ConstantDetail, Deprecation, Detail, ExtensionRecord, FunctionDetail,
    Kind, MethodDetail, Param, PropertyDetail, Record, Signature, Visibility,
};
use crate::version::VersionMask;

/// The header line, including the format version.
pub const HEADER: &str = "bleedthrough-builtins 1";

/// Term: exempt
///
/// Why data text could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_structs,
    reason = "a plain record of line and message"
)]
pub struct FormatError {
    /// The 1-based line, or 0 when the problem is not tied to a line.
    pub line: usize,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for FormatError {}

fn bad(message: impl Into<String>) -> String {
    message.into()
}

// ---- escaping ----

fn escape(text: &str, delimiters: &[char]) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() || delimiters.contains(&c) => {
                out.push_str(&format!("\\x{:02x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                let code = u8::from_str_radix(&hex, 16)
                    .ok()
                    .filter(|_| hex.len() == 2 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                    .ok_or_else(|| bad(format!("bad escape `\\x{hex}`")))?;
                out.push(char::from(code));
            }
            other => return Err(bad(format!("bad escape `\\{}`", other.unwrap_or(' ')))),
        }
    }
    Ok(out)
}

// ---- writing ----

fn join(items: &[String], separator: char) -> String {
    items
        .iter()
        .map(|i| escape(i, &[separator]))
        .collect::<Vec<_>>()
        .join(&separator.to_string())
}

fn words(words: &[&str]) -> String {
    words.join(",")
}

fn param_text(p: &Param) -> String {
    let mut flags = String::new();
    for (on, letter) in [
        (p.by_ref, 'r'),
        (p.optional, 'o'),
        (p.variadic, 'v'),
        (p.sensitive, 's'),
    ] {
        if on {
            flags.push(letter);
        }
    }
    let delimiters = [';', '@'];
    [
        escape(&p.name, &delimiters),
        flags,
        escape(&p.type_text, &delimiters),
        escape(&p.doc_type, &delimiters),
        p.default
            .as_deref()
            .map_or(String::new(), |d| escape(d, &delimiters)),
    ]
    .join("@")
}

fn signature_fields(s: &Signature, out: &mut Vec<String>) {
    out.push(
        s.params
            .iter()
            .map(param_text)
            .collect::<Vec<_>>()
            .join(";"),
    );
    out.push(escape(&s.return_type, &[]));
    out.push(escape(&s.tentative_return_type, &[]));
    out.push(escape(&s.doc_return_type, &[]));
}

fn optional(text: Option<&String>) -> String {
    text.map_or(String::new(), |t| escape(t, &[]))
}

fn visibility_flags<'a>(v: Visibility, rest: &[(bool, &'a str)]) -> Vec<&'a str> {
    let mut out = vec![v.as_str()];
    out.extend(rest.iter().filter(|(on, _)| *on).map(|(_, w)| *w));
    out
}

fn detail_fields(detail: &Detail) -> Vec<String> {
    let mut out = Vec::new();
    match detail {
        Detail::Constant(d) => {
            out.push(escape(&d.type_text, &[]));
            out.push(optional(d.value.as_ref()));
        }
        Detail::Function(d) => {
            let flags = [
                (d.signature.return_by_ref, "byref"),
                (d.no_discard, "nodiscard"),
            ];
            out.push(words(
                &flags
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, w)| *w)
                    .collect::<Vec<_>>(),
            ));
            out.push(optional(d.alias_of.as_ref()));
            signature_fields(&d.signature, &mut out);
        }
        Detail::Method(d) => {
            let flags = visibility_flags(
                d.visibility,
                &[
                    (d.is_static, "static"),
                    (d.is_final, "final"),
                    (d.is_abstract, "abstract"),
                    (d.signature.return_by_ref, "byref"),
                    (d.no_discard, "nodiscard"),
                ],
            );
            out.push(words(&flags));
            out.push(optional(d.alias_of.as_ref()));
            signature_fields(&d.signature, &mut out);
        }
        Detail::Class(d) | Detail::Interface(d) | Detail::Trait(d) | Detail::Enum(d) => {
            let flags = [
                (d.is_abstract, "abstract"),
                (d.is_final, "final"),
                (d.is_readonly, "readonly"),
            ];
            out.push(words(
                &flags
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, w)| *w)
                    .collect::<Vec<_>>(),
            ));
            out.push(join(&d.extends, ','));
            out.push(join(&d.implements, ','));
            out.push(escape(&d.backing_type, &[]));
        }
        Detail::ClassConstant(d) => {
            out.push(words(&visibility_flags(
                d.visibility,
                &[(d.is_final, "final")],
            )));
            out.push(escape(&d.type_text, &[]));
            out.push(optional(d.value.as_ref()));
        }
        Detail::EnumCase(d) => out.push(optional(d.value.as_ref())),
        Detail::Property(d) => {
            let flags = visibility_flags(
                d.visibility,
                &[
                    (d.is_static, "static"),
                    (d.is_readonly, "readonly"),
                    (d.is_virtual, "virtual"),
                ],
            );
            out.push(words(&flags));
            out.push(escape(&d.type_text, &[]));
            out.push(escape(&d.doc_type, &[]));
            out.push(optional(d.default.as_ref()));
        }
    }
    out
}

fn record_line(r: &Record) -> String {
    let deprecation = r.deprecation.as_ref().map_or(String::new(), |d| {
        format!("{}:{}", escape(&d.since, &[':']), escape(&d.message, &[]))
    });
    let mut fields = vec![
        r.kind().as_str().to_string(),
        escape(&r.name, &[]),
        r.mask.to_string(),
        r.reflected.to_string(),
        escape(&r.extension, &[]),
        join(&r.sapi, ','),
        join(&r.guards, ';'),
        deprecation,
    ];
    fields.extend(detail_fields(&r.detail));
    fields.join("\t")
}

fn extension_line(e: &ExtensionRecord) -> String {
    format!("extension\t{}\t{}", escape(&e.name, &[]), e.unbundled)
}

/// Writes the data in the canonical text format.
pub fn write(data: &BuiltinData) -> String {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    for extension in data.extensions() {
        out.push_str(&extension_line(extension));
        out.push('\n');
    }
    for record in data.records() {
        out.push_str(&record_line(record));
        out.push('\n');
    }
    out
}

// ---- reading ----

fn split_list(text: &str, separator: char) -> Result<Vec<String>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(separator).map(unescape).collect()
}

fn flag_words<'a>(text: &'a str, vocabulary: &[&str]) -> Result<Vec<&'a str>, String> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|w| {
            if vocabulary.contains(&w) {
                Ok(w)
            } else {
                Err(bad(format!("unknown flag `{w}`")))
            }
        })
        .collect()
}

fn optional_text(text: &str) -> Result<Option<String>, String> {
    if text.is_empty() {
        Ok(None)
    } else {
        unescape(text).map(Some)
    }
}

fn parse_param(text: &str) -> Result<Param, String> {
    let parts: Vec<&str> = text.split('@').collect();
    let [name, flags, type_text, doc_type, default] = parts[..] else {
        return Err(bad(format!(
            "a parameter has {} parts, expected 5",
            parts.len()
        )));
    };
    let mut param = Param {
        name: unescape(name)?,
        type_text: unescape(type_text)?,
        doc_type: unescape(doc_type)?,
        default: optional_text(default)?,
        ..Param::default()
    };
    for letter in flags.chars() {
        match letter {
            'r' => param.by_ref = true,
            'o' => param.optional = true,
            'v' => param.variadic = true,
            's' => param.sensitive = true,
            other => return Err(bad(format!("unknown parameter flag `{other}`"))),
        }
    }
    Ok(param)
}

fn parse_visibility(words: &[&str]) -> Result<Visibility, String> {
    match words.first() {
        Some(&"public") => Ok(Visibility::Public),
        Some(&"protected") => Ok(Visibility::Protected),
        Some(&"private") => Ok(Visibility::Private),
        _ => Err(bad("the flags must start with a visibility")),
    }
}

struct Fields<'a> {
    items: std::slice::Iter<'a, &'a str>,
}

impl<'a> Fields<'a> {
    fn next(&mut self) -> &'a str {
        self.items.next().copied().unwrap_or_default()
    }
}

fn parse_signature(fields: &mut Fields<'_>) -> Result<Signature, String> {
    let params = fields.next();
    Ok(Signature {
        params: if params.is_empty() {
            Vec::new()
        } else {
            params
                .split(';')
                .map(parse_param)
                .collect::<Result<_, _>>()?
        },
        return_type: unescape(fields.next())?,
        tentative_return_type: unescape(fields.next())?,
        doc_return_type: unescape(fields.next())?,
        return_by_ref: false,
    })
}

fn detail_width(kind: Kind) -> usize {
    match kind {
        Kind::Constant => 2,
        Kind::Function | Kind::Method => 6,
        Kind::Class | Kind::Interface | Kind::Trait | Kind::Enum => 4,
        Kind::ClassConstant => 3,
        Kind::EnumCase => 1,
        Kind::Property => 4,
    }
}

fn parse_function(fields: &mut Fields<'_>) -> Result<Detail, String> {
    let flags = flag_words(fields.next(), &["byref", "nodiscard"])?;
    let alias_of = optional_text(fields.next())?;
    let mut signature = parse_signature(fields)?;
    signature.return_by_ref = flags.contains(&"byref");
    Ok(Detail::Function(FunctionDetail {
        signature,
        alias_of,
        no_discard: flags.contains(&"nodiscard"),
    }))
}

fn parse_method(fields: &mut Fields<'_>) -> Result<Detail, String> {
    const VOCABULARY: [&str; 8] = [
        "public",
        "protected",
        "private",
        "static",
        "final",
        "abstract",
        "byref",
        "nodiscard",
    ];
    let flags = flag_words(fields.next(), &VOCABULARY)?;
    let alias_of = optional_text(fields.next())?;
    let mut signature = parse_signature(fields)?;
    signature.return_by_ref = flags.contains(&"byref");
    Ok(Detail::Method(MethodDetail {
        signature,
        alias_of,
        no_discard: flags.contains(&"nodiscard"),
        visibility: parse_visibility(&flags)?,
        is_static: flags.contains(&"static"),
        is_final: flags.contains(&"final"),
        is_abstract: flags.contains(&"abstract"),
    }))
}

fn parse_class(kind: Kind, fields: &mut Fields<'_>) -> Result<Detail, String> {
    let flags = flag_words(fields.next(), &["abstract", "final", "readonly"])?;
    let class = ClassDetail {
        is_abstract: flags.contains(&"abstract"),
        is_final: flags.contains(&"final"),
        is_readonly: flags.contains(&"readonly"),
        extends: split_list(fields.next(), ',')?,
        implements: split_list(fields.next(), ',')?,
        backing_type: unescape(fields.next())?,
    };
    Ok(match kind {
        Kind::Class => Detail::Class(class),
        Kind::Interface => Detail::Interface(class),
        Kind::Trait => Detail::Trait(class),
        _ => Detail::Enum(class),
    })
}

fn parse_class_constant(fields: &mut Fields<'_>) -> Result<Detail, String> {
    let flags = flag_words(fields.next(), &["public", "protected", "private", "final"])?;
    Ok(Detail::ClassConstant(ConstantDetail {
        visibility: parse_visibility(&flags)?,
        is_final: flags.contains(&"final"),
        type_text: unescape(fields.next())?,
        value: optional_text(fields.next())?,
    }))
}

fn parse_property(fields: &mut Fields<'_>) -> Result<Detail, String> {
    const VOCABULARY: [&str; 6] = [
        "public",
        "protected",
        "private",
        "static",
        "readonly",
        "virtual",
    ];
    let flags = flag_words(fields.next(), &VOCABULARY)?;
    Ok(Detail::Property(PropertyDetail {
        visibility: parse_visibility(&flags)?,
        is_static: flags.contains(&"static"),
        is_readonly: flags.contains(&"readonly"),
        is_virtual: flags.contains(&"virtual"),
        type_text: unescape(fields.next())?,
        doc_type: unescape(fields.next())?,
        default: optional_text(fields.next())?,
    }))
}

fn parse_detail(kind: Kind, fields: &mut Fields<'_>) -> Result<Detail, String> {
    match kind {
        Kind::Constant => Ok(Detail::Constant(ConstantDetail {
            type_text: unescape(fields.next())?,
            value: optional_text(fields.next())?,
            ..ConstantDetail::default()
        })),
        Kind::Function => parse_function(fields),
        Kind::Method => parse_method(fields),
        Kind::Class | Kind::Interface | Kind::Trait | Kind::Enum => parse_class(kind, fields),
        Kind::ClassConstant => parse_class_constant(fields),
        Kind::EnumCase => Ok(Detail::EnumCase(CaseDetail {
            value: optional_text(fields.next())?,
        })),
        Kind::Property => parse_property(fields),
    }
}

fn parse_mask(text: &str) -> Result<VersionMask, String> {
    VersionMask::parse(text).map_err(|e| e.to_string())
}

fn parse_record(line: &str) -> Result<Record, String> {
    let all: Vec<&str> = line.split('\t').collect();
    let kind =
        Kind::parse(all[0]).ok_or_else(|| bad(format!("unknown record kind `{}`", all[0])))?;
    let expected = 8 + detail_width(kind);
    if all.len() != expected {
        return Err(bad(format!(
            "a {} record has {} fields, expected {expected}",
            kind.as_str(),
            all.len()
        )));
    }
    let deprecation = if all[7].is_empty() {
        None
    } else {
        let (since, message) = all[7]
            .split_once(':')
            .ok_or_else(|| bad("a deprecation is `since:message`"))?;
        Some(Deprecation {
            since: unescape(since)?,
            message: unescape(message)?,
        })
    };
    let mut fields = Fields {
        items: all[8..].iter(),
    };
    Ok(Record {
        name: unescape(all[1])?,
        mask: parse_mask(all[2])?,
        reflected: parse_mask(all[3])?,
        extension: unescape(all[4])?,
        sapi: split_list(all[5], ',')?,
        guards: split_list(all[6], ';')?,
        deprecation,
        detail: parse_detail(kind, &mut fields)?,
    })
}

fn parse_extension(line: &str) -> Result<ExtensionRecord, String> {
    let parts: Vec<&str> = line.split('\t').collect();
    let [_, name, unbundled] = parts[..] else {
        return Err(bad(format!(
            "an extension line has {} fields, expected 3",
            parts.len()
        )));
    };
    Ok(ExtensionRecord {
        name: unescape(name)?,
        unbundled: parse_mask(unbundled)?,
    })
}

/// Reads data in the canonical text format. Anything the writer would not produce is rejected
/// with its line number: an unsupported version, unknown kinds, wrong field counts, bad escapes,
/// invalid fields, records out of order, duplicates, overlapping masks and non-canonical spelling.
pub fn read(text: &str) -> Result<BuiltinData, FormatError> {
    let fail = |line: usize, message: String| FormatError { line, message };
    if !text.ends_with('\n') && !text.is_empty() {
        return Err(fail(0, "the data must end with a newline".to_string()));
    }
    let mut lines = text.split_terminator('\n').enumerate();
    match lines.next() {
        Some((_, HEADER)) => {}
        Some((_, other)) if other.starts_with("bleedthrough-builtins ") => {
            return Err(fail(
                1,
                format!("unsupported format `{other}`, expected `{HEADER}`"),
            ));
        }
        _ => return Err(fail(1, format!("expected the header `{HEADER}`"))),
    }
    let mut extensions: Vec<ExtensionRecord> = Vec::new();
    let mut records: Vec<Record> = Vec::new();
    let mut run: Vec<VersionMask> = Vec::new();
    for (index, line) in lines {
        let number = index + 1;
        if line.starts_with("extension\t") {
            if !records.is_empty() {
                return Err(fail(
                    number,
                    "extension lines must precede records".to_string(),
                ));
            }
            let extension = parse_extension(line).map_err(|m| fail(number, m))?;
            if extensions.last().is_some_and(|p| p.name >= extension.name) {
                return Err(fail(
                    number,
                    format!("extension `{}` is out of order or repeated", extension.name),
                ));
            }
            if extension_line(&extension) != line {
                return Err(fail(number, "not in canonical form".to_string()));
            }
            extensions.push(extension);
            continue;
        }
        let record = parse_record(line).map_err(|m| fail(number, m))?;
        crate::validate::check_record(&record)
            .map_err(|m| fail(number, format!("{}: {m}", record.name)))?;
        if record_line(&record) != line {
            return Err(fail(number, "not in canonical form".to_string()));
        }
        if let Some(previous) = records.last() {
            if previous.key() == record.key() {
                if run.iter().any(|m| m.overlaps(record.mask)) {
                    return Err(fail(
                        number,
                        format!(
                            "{}: version masks overlap an earlier record or repeat it",
                            record.name
                        ),
                    ));
                }
            } else {
                run.clear();
            }
            if canonical_order(previous, &record).is_ge() {
                return Err(fail(
                    number,
                    format!("{}: records are out of order", record.name),
                ));
            }
        }
        run.push(record.mask);
        records.push(record);
    }
    BuiltinData::new(extensions, records).map_err(|e| fail(0, e.to_string()))
}
