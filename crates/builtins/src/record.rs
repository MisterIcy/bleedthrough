//! The records of the built-in data: one per symbol and distinct signature.

use crate::version::VersionMask;

/// Term: Built-in data
///
/// The kind of a record. The declaration order is the canonical sort order of the data file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// A global constant.
    Constant,
    /// A function.
    Function,
    /// A class.
    Class,
    /// An interface.
    Interface,
    /// A trait.
    Trait,
    /// An enum.
    Enum,
    /// A class, interface or trait constant.
    ClassConstant,
    /// An enum case.
    EnumCase,
    /// A property.
    Property,
    /// A method.
    Method,
}

impl Kind {
    /// Every kind in canonical order.
    pub const ALL: [Kind; 10] = [
        Kind::Constant,
        Kind::Function,
        Kind::Class,
        Kind::Interface,
        Kind::Trait,
        Kind::Enum,
        Kind::ClassConstant,
        Kind::EnumCase,
        Kind::Property,
        Kind::Method,
    ];

    /// The spelling used in the data file.
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Constant => "const",
            Kind::Function => "function",
            Kind::Class => "class",
            Kind::Interface => "interface",
            Kind::Trait => "trait",
            Kind::Enum => "enum",
            Kind::ClassConstant => "classconst",
            Kind::EnumCase => "case",
            Kind::Property => "property",
            Kind::Method => "method",
        }
    }

    /// Reads the data-file spelling.
    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.as_str() == text)
    }

    /// Whether PHP compares names of this kind without regard to case. Functions, classes and
    /// methods are case-insensitive; constants, properties and cases are not.
    pub const fn is_case_insensitive(self) -> bool {
        matches!(
            self,
            Kind::Function
                | Kind::Class
                | Kind::Interface
                | Kind::Trait
                | Kind::Enum
                | Kind::Method
        )
    }

    /// Whether the kind names a member of a class-like (`Class::member`).
    pub const fn is_member(self) -> bool {
        matches!(
            self,
            Kind::ClassConstant | Kind::EnumCase | Kind::Property | Kind::Method
        )
    }

    /// Whether the kind is a class-like.
    pub const fn is_class_like(self) -> bool {
        matches!(
            self,
            Kind::Class | Kind::Interface | Kind::Trait | Kind::Enum
        )
    }
}

/// Term: Built-in data
///
/// The visibility of a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[non_exhaustive]
pub enum Visibility {
    /// `public`
    #[default]
    Public,
    /// `protected`
    Protected,
    /// `private`
    Private,
}

impl Visibility {
    /// The PHP keyword.
    pub const fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        }
    }
}

/// Term: Built-in data
///
/// A function or method parameter. Type and default are PHP source text; an empty type means the
/// parameter is untyped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Param {
    /// The name without `$`.
    pub name: String,
    /// The declared type.
    pub type_text: String,
    /// The type from the docblock when it differs from the declared one.
    pub doc_type: String,
    /// Taken by reference.
    pub by_ref: bool,
    /// Has a default or is variadic, so may be omitted.
    pub optional: bool,
    /// The default expression as source text; none for a required parameter and for a default the
    /// stub leaves out.
    pub default: Option<String>,
    /// `...$name`
    pub variadic: bool,
    /// Marked `#[\SensitiveParameter]`.
    pub sensitive: bool,
}

/// Term: Built-in data
///
/// The parameters and return of a function or method.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Signature {
    /// The parameters in order.
    pub params: Vec<Param>,
    /// The declared return type; empty when there is none.
    pub return_type: String,
    /// The tentative return type; when set, the declared one is empty.
    pub tentative_return_type: String,
    /// The return type from the docblock when it differs.
    pub doc_return_type: String,
    /// Returns by reference (`function &f()`).
    pub return_by_ref: bool,
}

/// Term: Built-in data
///
/// A deprecation. An empty `since` or `message` means it is not known.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Deprecation {
    /// The version that deprecated the symbol, such as `8.4`.
    pub since: String,
    /// The short message from the attribute.
    pub message: String,
}

/// Term: Built-in data
///
/// The details of a function record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct FunctionDetail {
    /// Parameters and return.
    pub signature: Signature,
    /// The function this one is an alias of, as written in the stub.
    pub alias_of: Option<String>,
    /// Marked `#[\NoDiscard]`.
    pub no_discard: bool,
}

/// Term: Built-in data
///
/// The details of a method record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct MethodDetail {
    /// Parameters and return.
    pub signature: Signature,
    /// The method this one is an alias of (`Class::method`).
    pub alias_of: Option<String>,
    /// Marked `#[\NoDiscard]`.
    pub no_discard: bool,
    /// The visibility.
    pub visibility: Visibility,
    /// `static`
    pub is_static: bool,
    /// `final`
    pub is_final: bool,
    /// `abstract`
    pub is_abstract: bool,
}

/// Term: Built-in data
///
/// The details of a class, interface, trait or enum record. The kind of the record says which.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ClassDetail {
    /// `abstract`
    pub is_abstract: bool,
    /// `final`
    pub is_final: bool,
    /// `readonly`
    pub is_readonly: bool,
    /// The parent class, or the parent interfaces of an interface.
    pub extends: Vec<String>,
    /// The implemented interfaces.
    pub implements: Vec<String>,
    /// The backing type of a backed enum; empty otherwise.
    pub backing_type: String,
}

/// Term: Built-in data
///
/// The details of a property record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct PropertyDetail {
    /// The declared type.
    pub type_text: String,
    /// The type from the docblock when it differs.
    pub doc_type: String,
    /// The default expression as source text.
    pub default: Option<String>,
    /// The visibility.
    pub visibility: Visibility,
    /// `static`
    pub is_static: bool,
    /// `readonly`
    pub is_readonly: bool,
    /// Marked `@virtual`: not a stored property.
    pub is_virtual: bool,
}

/// Term: Built-in data
///
/// The details of a global or class constant record. Values are never in the stubs; they come
/// from the Reflection fixtures and are written as PHP literal text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ConstantDetail {
    /// The declared type, when the stub gives one.
    pub type_text: String,
    /// The value as PHP literal text, when known.
    pub value: Option<String>,
    /// The visibility (always public for a global constant).
    pub visibility: Visibility,
    /// `final`
    pub is_final: bool,
}

/// Term: Built-in data
///
/// The details of an enum case record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct CaseDetail {
    /// The backing value as PHP literal text, for a backed enum.
    pub value: Option<String>,
}

/// Term: Built-in data
///
/// What is particular to a record's kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Detail {
    /// A global constant.
    Constant(ConstantDetail),
    /// A function.
    Function(FunctionDetail),
    /// A class.
    Class(ClassDetail),
    /// An interface.
    Interface(ClassDetail),
    /// A trait.
    Trait(ClassDetail),
    /// An enum.
    Enum(ClassDetail),
    /// A class constant.
    ClassConstant(ConstantDetail),
    /// An enum case.
    EnumCase(CaseDetail),
    /// A property.
    Property(PropertyDetail),
    /// A method.
    Method(MethodDetail),
}

impl Detail {
    /// The kind of record this detail belongs to.
    pub const fn kind(&self) -> Kind {
        match self {
            Detail::Constant(_) => Kind::Constant,
            Detail::Function(_) => Kind::Function,
            Detail::Class(_) => Kind::Class,
            Detail::Interface(_) => Kind::Interface,
            Detail::Trait(_) => Kind::Trait,
            Detail::Enum(_) => Kind::Enum,
            Detail::ClassConstant(_) => Kind::ClassConstant,
            Detail::EnumCase(_) => Kind::EnumCase,
            Detail::Property(_) => Kind::Property,
            Detail::Method(_) => Kind::Method,
        }
    }
}

/// Term: Built-in data
///
/// One built-in symbol for the versions in its mask. A symbol whose signature differs between
/// versions has one record per distinct form, with non-overlapping masks.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct Record {
    /// The fully qualified name without a leading backslash; `Class::member` for a member.
    pub name: String,
    /// The versions this record applies to.
    pub mask: VersionMask,
    /// The versions, a subset of the mask, for which the record (or its constant value) was taken
    /// from the Reflection fixtures instead of the stubs.
    pub reflected: VersionMask,
    /// The owning extension, in lower case (`standard`, `core`, `pdo`).
    pub extension: String,
    /// The SAPIs that provide the symbol, sorted; empty when every SAPI does.
    pub sapi: Vec<String>,
    /// The build conditions under which the symbol exists, as alternatives (any one suffices);
    /// empty when unconditional.
    pub guards: Vec<String>,
    /// The deprecation, if any.
    pub deprecation: Option<Deprecation>,
    /// The kind-specific details.
    pub detail: Detail,
}

impl Record {
    /// A record with no SAPI, guard, deprecation or Reflection origin.
    pub fn new(
        name: impl Into<String>,
        mask: VersionMask,
        extension: impl Into<String>,
        detail: Detail,
    ) -> Self {
        Self {
            name: name.into(),
            mask,
            reflected: VersionMask::EMPTY,
            extension: extension.into(),
            sapi: Vec::new(),
            guards: Vec::new(),
            deprecation: None,
            detail,
        }
    }

    /// The kind of the record.
    pub const fn kind(&self) -> Kind {
        self.detail.kind()
    }

    /// The identity of the symbol: the kind and the name as PHP compares it, so records of one
    /// symbol share a key and differ only in mask.
    pub fn key(&self) -> (Kind, String) {
        (self.kind(), normalise_name(self.kind(), &self.name))
    }

    /// The class part of a member name (`Class` of `Class::member`).
    pub fn owner(&self) -> Option<&str> {
        if self.kind().is_member() {
            self.name.split_once("::").map(|(class, _)| class)
        } else {
            None
        }
    }
}

/// A name as PHP compares it for `kind`: lower-cased where PHP ignores case. For a member, the
/// class part is always lower-cased and the member part follows the member's kind.
pub fn normalise_name(kind: Kind, name: &str) -> String {
    let name = name.strip_prefix('\\').unwrap_or(name);
    if let (true, Some((class, member))) = (kind.is_member(), name.split_once("::")) {
        let member = if kind.is_case_insensitive() {
            member.to_ascii_lowercase()
        } else {
            member.to_string()
        };
        return format!("{}::{member}", class.to_ascii_lowercase());
    }
    if kind.is_case_insensitive() {
        name.to_ascii_lowercase()
    } else {
        name.to_string()
    }
}

/// Term: Built-in data
///
/// What an extension's bundling status adds to the data: the versions in which a once-bundled
/// extension is no longer part of the PHP distribution (moved to PECL, for example). Its symbols
/// are then unbundled rather than removed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ExtensionRecord {
    /// The extension name in lower case.
    pub name: String,
    /// The versions in which the extension is not bundled.
    pub unbundled: VersionMask,
}
