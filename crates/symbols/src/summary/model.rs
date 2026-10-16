//! The records a file summary holds: plain, owned data with no reference to other files.

use bleedthrough_syntax::Span;

use crate::diag::SymbolDiagnostic;
use crate::name::Fqn;
use crate::native::EffectiveType;
use crate::types::Type;

/// Term: exempt
///
/// Member visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[non_exhaustive]
pub enum Visibility {
    /// Visible everywhere.
    #[default]
    Public,
    /// Visible in the class and its subclasses.
    Protected,
    /// Visible in the class only.
    Private,
}

impl Visibility {
    /// The stable lowercase spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Protected => "protected",
            Self::Private => "private",
        }
    }
}

/// Term: exempt
///
/// Where a member comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum MemberOrigin {
    /// Declared in code.
    Declared,
    /// A promoted constructor parameter.
    Promoted,
    /// Declared only by a docblock `@property` or `@method` tag.
    Docblock,
}

impl MemberOrigin {
    /// The stable lowercase spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Promoted => "promoted",
            Self::Docblock => "docblock",
        }
    }
}

/// Term: exempt
///
/// Which kind of class-like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ClassKind {
    /// `class`
    Class,
    /// `interface`
    Interface,
    /// `trait`
    Trait,
    /// `enum`
    Enum,
}

impl ClassKind {
    /// The stable lowercase spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Enum => "enum",
        }
    }
}

/// Term: File summary
///
/// What is known about a built-in symbol beyond its declaration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct BuiltinMeta {
    /// The PHP extension that provides it.
    pub extension: Box<str>,
    /// Build conditions under which it exists, as C preprocessor text.
    pub guards: Vec<Box<str>>,
    /// Whether it is deprecated in the modelled version.
    pub deprecated: bool,
}

/// Term: File summary
///
/// A parameter of a function or method.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ParamDecl {
    /// The name without `$`.
    pub name: Box<str>,
    /// Its effective type (variadic parameters: the type of each element).
    pub ty: EffectiveType,
    /// Whether it is taken by reference.
    pub by_ref: bool,
    /// Whether it collects the remaining arguments.
    pub variadic: bool,
    /// Whether it has a default (it may be omitted).
    pub has_default: bool,
    /// Attributes, resolved.
    pub attributes: Vec<Fqn>,
}

/// Term: File summary
///
/// A function.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct FunctionDecl {
    /// The fully qualified name.
    pub name: Fqn,
    /// Where the declaration is.
    pub span: Span,
    /// Parameters in order.
    pub params: Vec<ParamDecl>,
    /// The effective return type.
    pub ret: EffectiveType,
    /// Whether it returns by reference.
    pub by_ref: bool,
    /// Attributes, resolved.
    pub attributes: Vec<Fqn>,
    /// The raw docblock.
    pub docblock: Option<Box<str>>,
    /// Template parameters declared by the docblock.
    pub templates: Vec<Box<str>>,
    /// Built-in information, for built-in functions.
    pub builtin: Option<BuiltinMeta>,
}

/// Term: File summary
///
/// A method.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct MethodDecl {
    /// The name as declared.
    pub name: Box<str>,
    /// Where the declaration is.
    pub span: Span,
    /// Visibility.
    pub visibility: Visibility,
    /// Whether it is static.
    pub is_static: bool,
    /// Whether it is final.
    pub is_final: bool,
    /// Whether it has no body (abstract or interface method).
    pub is_abstract: bool,
    /// Parameters in order.
    pub params: Vec<ParamDecl>,
    /// The effective return type.
    pub ret: EffectiveType,
    /// Whether it returns by reference.
    pub by_ref: bool,
    /// Declared in code or only by a docblock.
    pub origin: MemberOrigin,
    /// Attributes, resolved.
    pub attributes: Vec<Fqn>,
    /// The raw docblock.
    pub docblock: Option<Box<str>>,
    /// Template parameters declared by the docblock.
    pub templates: Vec<Box<str>>,
}

/// Term: File summary
///
/// A property, including promoted constructor parameters and docblock-declared ones.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct PropertyDecl {
    /// The name without `$`.
    pub name: Box<str>,
    /// Where the declaration is.
    pub span: Span,
    /// Visibility for reading.
    pub visibility: Visibility,
    /// Visibility for writing when it differs (`private(set)`).
    pub set_visibility: Option<Visibility>,
    /// Whether it is static.
    pub is_static: bool,
    /// Whether it is readonly.
    pub is_readonly: bool,
    /// The effective type.
    pub ty: EffectiveType,
    /// Whether a default value is declared.
    pub has_default: bool,
    /// Names of the property hooks (`get`, `set`).
    pub hooks: Vec<Box<str>>,
    /// Declared, promoted or docblock-declared.
    pub origin: MemberOrigin,
    /// Attributes, resolved.
    pub attributes: Vec<Fqn>,
}

/// Term: File summary
///
/// A class constant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ConstDecl {
    /// The name.
    pub name: Box<str>,
    /// Where the declaration is.
    pub span: Span,
    /// Visibility.
    pub visibility: Visibility,
    /// Whether it is final.
    pub is_final: bool,
    /// The declared type, or the type of the literal value, or unknown.
    pub ty: Type,
}

/// Term: File summary
///
/// An enum case.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct CaseDecl {
    /// The case name.
    pub name: Box<str>,
    /// Where the declaration is.
    pub span: Span,
    /// The type of the backing value for a literal one, else `None`.
    pub value: Option<Type>,
}

/// Term: exempt
///
/// A trait adaptation inside a `use` of traits.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Adaptation {
    /// `T::m insteadof U, V;`
    InsteadOf {
        /// The trait whose method is used.
        trait_name: Fqn,
        /// The method.
        method: Box<str>,
        /// The traits whose method is excluded.
        excluded: Vec<Fqn>,
    },
    /// `m as protected alias;`
    Alias {
        /// The trait, when named.
        trait_name: Option<Fqn>,
        /// The original method.
        method: Box<str>,
        /// A new visibility.
        visibility: Option<Visibility>,
        /// A new name.
        alias: Option<Box<str>>,
    },
}

/// Term: File summary
///
/// One `use T1, T2 { ... }` of traits inside a class-like.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct TraitUse {
    /// The traits.
    pub traits: Vec<Fqn>,
    /// The adaptations.
    pub adaptations: Vec<Adaptation>,
}

/// Term: File summary
///
/// A class, interface, trait or enum.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ClassDecl {
    /// The fully qualified name.
    pub name: Fqn,
    /// Class, interface, trait or enum.
    pub kind: ClassKind,
    /// Where the declaration is.
    pub span: Span,
    /// Whether it is abstract.
    pub is_abstract: bool,
    /// Whether it is final.
    pub is_final: bool,
    /// Whether it is readonly.
    pub is_readonly: bool,
    /// The parent class.
    pub parent: Option<Fqn>,
    /// Implemented interfaces (for an interface: the interfaces it extends).
    pub interfaces: Vec<Fqn>,
    /// Trait uses in order.
    pub trait_uses: Vec<TraitUse>,
    /// The backing type of an enum.
    pub backing_type: Option<Type>,
    /// Class constants.
    pub constants: Vec<ConstDecl>,
    /// Properties, including promoted and docblock ones.
    pub properties: Vec<PropertyDecl>,
    /// Methods, including docblock ones.
    pub methods: Vec<MethodDecl>,
    /// Enum cases.
    pub cases: Vec<CaseDecl>,
    /// Template parameters declared by the class docblock.
    pub templates: Vec<Box<str>>,
    /// Types named by `@extends`, `@implements` and `@use` with their arguments.
    pub inherits: Vec<Type>,
    /// Attributes, resolved.
    pub attributes: Vec<Fqn>,
    /// The raw docblock.
    pub docblock: Option<Box<str>>,
    /// Built-in information, for built-in classes.
    pub builtin: Option<BuiltinMeta>,
}

/// Term: exempt
///
/// How a global constant was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ConstantVia {
    /// `const NAME = ...;`
    Const,
    /// `define('NAME', ...)`
    Define,
}

/// Term: File summary
///
/// A global constant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct ConstantDecl {
    /// The fully qualified name.
    pub name: Fqn,
    /// Where the declaration is.
    pub span: Span,
    /// The type of the value when it is a literal, else unknown.
    pub ty: Type,
    /// How it was declared.
    pub via: ConstantVia,
    /// Built-in information, for built-in constants.
    pub builtin: Option<BuiltinMeta>,
}

/// Term: File summary
///
/// A function the file calls, as the candidates PHP considers: the namespaced name (when the call
/// was unqualified inside a namespace) and the global one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct FunctionRef {
    /// The first candidate.
    pub primary: Fqn,
    /// The fallback, when the call could resolve to either.
    pub fallback: Option<Fqn>,
}

/// Term: File summary
///
/// The declarations of one file and the names it refers to, a pure function of its syntax tree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::exhaustive_structs, reason = "a plain record")]
pub struct FileSummary {
    /// The path the file is known by (project-relative or vendor-relative).
    pub path: Box<str>,
    /// Classes, interfaces, traits and enums in source order.
    pub classes: Vec<ClassDecl>,
    /// Functions in source order.
    pub functions: Vec<FunctionDecl>,
    /// Global constants in source order.
    pub constants: Vec<ConstantDecl>,
    /// Every class name the file refers to (declarations and bodies), sorted and distinct.
    pub referenced_classes: Vec<Fqn>,
    /// The class names mentioned by declarations only (hierarchy, attributes and member types),
    /// sorted and distinct: what a dependency needs loaded for its declared interface.
    pub declaration_classes: Vec<Fqn>,
    /// Every function the file calls by name, sorted and distinct.
    pub referenced_functions: Vec<FunctionRef>,
    /// Problems found in names and docblocks.
    pub diagnostics: Vec<SymbolDiagnostic>,
    /// Whether the tree contained error or unsupported nodes (the summary may be incomplete).
    pub incomplete: bool,
}
