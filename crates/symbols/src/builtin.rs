//! PHP's own functions, classes and constants, converted from the built-in data for one modelled
//! PHP version. The data arrives through the `BuiltinDataSource` port; nothing here reads files.

use bleedthrough_builtins::{
    BuiltinData, BuiltinDataSource, ClassDetail, Detail, Kind, MethodDetail, Param, PhpVersion,
    Record, Signature, SourceError, Visibility as DataVisibility,
};
use bleedthrough_syntax::{ClassLikeKind, Span};

use crate::diag::{SymbolDiagnostic, SymbolDiagnosticCode};
use crate::docblock::{TypeContext, parse_type};
use crate::name::Fqn;
use crate::native::{EffectiveType, merge_types};
use crate::special::ClassContext;
use crate::summary::{
    BuiltinMeta, CaseDecl, ClassDecl, ClassKind, ConstDecl, ConstantDecl, ConstantVia, FileSummary,
    FunctionDecl, MemberOrigin, MethodDecl, ParamDecl, PropertyDecl, Visibility,
};
use crate::types::Type;

/// The path the built-in summary carries.
pub const BUILTIN_PATH: &str = "<builtin>";

/// Term: Built-in data
///
/// Whether a built-in symbol exists for the modelled PHP version.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Availability {
    /// It exists. `guards` are build conditions under which it may still be absent.
    Available {
        /// The providing extension.
        extension: Box<str>,
        /// Build conditions, any one of which suffices; empty when unconditional.
        guards: Vec<Box<str>>,
        /// Whether it is deprecated in the modelled version.
        deprecated: bool,
    },
    /// It exists in other versions but not in the modelled one.
    NotInVersion,
    /// Its extension is not bundled with the modelled version.
    Unbundled {
        /// The extension.
        extension: Box<str>,
    },
    /// No version of PHP has it.
    Unknown,
}

/// Term: Built-in data
///
/// The built-in data for one modelled PHP version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Builtins {
    data: BuiltinData,
    version: PhpVersion,
}

impl Builtins {
    /// Wraps data already loaded.
    pub fn new(data: BuiltinData, version: PhpVersion) -> Self {
        Self { data, version }
    }

    /// Loads the data from a source.
    pub fn load(source: &dyn BuiltinDataSource, version: PhpVersion) -> Result<Self, SourceError> {
        source.load().map(|data| Self::new(data, version))
    }

    /// The modelled version.
    pub fn version(&self) -> PhpVersion {
        self.version
    }

    /// The underlying data.
    pub fn data(&self) -> &BuiltinData {
        &self.data
    }

    /// Whether the function exists for the modelled version.
    pub fn function(&self, name: &Fqn) -> Availability {
        self.availability(&[Kind::Function], name.as_str())
    }

    /// Whether the class, interface, trait or enum exists for the modelled version.
    pub fn class(&self, name: &Fqn) -> Availability {
        self.availability(
            &[Kind::Class, Kind::Interface, Kind::Trait, Kind::Enum],
            name.as_str(),
        )
    }

    /// Whether the global constant exists for the modelled version.
    pub fn constant(&self, name: &Fqn) -> Availability {
        self.availability(&[Kind::Constant], name.as_str())
    }

    fn availability(&self, kinds: &[Kind], name: &str) -> Availability {
        use bleedthrough_builtins::Lookup;
        let mut best = Availability::Unknown;
        for kind in kinds {
            match self.data.lookup(*kind, name, self.version) {
                Lookup::Found(record) => return available(record),
                Lookup::Unbundled => {
                    let extension = self
                        .data
                        .symbol(*kind, name)
                        .first()
                        .map_or_else(Box::default, |r| r.extension.as_str().into());
                    best = Availability::Unbundled { extension };
                }
                Lookup::NotInVersion if best == Availability::Unknown => {
                    best = Availability::NotInVersion;
                }
                _ => {}
            }
        }
        best
    }

    /// Every symbol that exists for the modelled version, as one summary with built-in origin.
    pub fn summary(&self) -> FileSummary {
        let mut converter = Converter {
            diagnostics: Vec::new(),
        };
        let mut classes = Vec::new();
        let mut functions = Vec::new();
        let mut constants = Vec::new();
        for record in self.data.records() {
            if !record.mask.contains(self.version) {
                continue;
            }
            match &record.detail {
                Detail::Constant(detail) => constants.push(ConstantDecl {
                    name: Fqn::new(&record.name),
                    span: Span::new(0, 0),
                    ty: converter.native(&detail.type_text, &record.name),
                    via: ConstantVia::Const,
                    builtin: Some(meta(record)),
                }),
                Detail::Function(detail) => {
                    let (params, ret, by_ref) =
                        converter.signature(&detail.signature, &ClassContext::none(), &record.name);
                    functions.push(FunctionDecl {
                        name: Fqn::new(&record.name),
                        span: Span::new(0, 0),
                        params,
                        ret,
                        by_ref,
                        attributes: Vec::new(),
                        docblock: None,
                        templates: Vec::new(),
                        builtin: Some(meta(record)),
                    });
                }
                Detail::Class(d) => {
                    classes.push(self.class_decl(&mut converter, record, d, ClassKind::Class))
                }
                Detail::Interface(d) => {
                    classes.push(self.class_decl(&mut converter, record, d, ClassKind::Interface));
                }
                Detail::Trait(d) => {
                    classes.push(self.class_decl(&mut converter, record, d, ClassKind::Trait))
                }
                Detail::Enum(d) => {
                    classes.push(self.class_decl(&mut converter, record, d, ClassKind::Enum))
                }
                _ => {}
            }
        }
        let mut summary = FileSummary {
            path: BUILTIN_PATH.into(),
            classes,
            functions,
            constants,
            referenced_classes: Vec::new(),
            declaration_classes: Vec::new(),
            referenced_functions: Vec::new(),
            diagnostics: converter.diagnostics,
            incomplete: false,
        };
        summary.diagnostics.sort();
        let declared = crate::summary::declaration_classes(&summary);
        summary.referenced_classes = declared.clone();
        summary.declaration_classes = declared;
        summary
    }

    fn class_decl(
        &self,
        converter: &mut Converter,
        record: &Record,
        detail: &ClassDetail,
        kind: ClassKind,
    ) -> ClassDecl {
        let name = Fqn::new(&record.name);
        let first = |list: &[String]| list.first().map(|n| Fqn::new(n));
        let (parent, interfaces) = if kind == ClassKind::Interface {
            (None, detail.extends.iter().map(|n| Fqn::new(n)).collect())
        } else {
            (
                first(&detail.extends),
                detail.implements.iter().map(|n| Fqn::new(n)).collect(),
            )
        };
        let context = ClassContext::new(name.clone(), syntax_kind(kind), parent.clone());
        let mut decl = ClassDecl {
            name,
            kind,
            span: Span::new(0, 0),
            is_abstract: detail.is_abstract,
            is_final: detail.is_final,
            is_readonly: detail.is_readonly,
            parent,
            interfaces,
            trait_uses: Vec::new(),
            backing_type: (!detail.backing_type.is_empty())
                .then(|| converter.native(&detail.backing_type, &record.name)),
            constants: Vec::new(),
            properties: Vec::new(),
            methods: Vec::new(),
            cases: Vec::new(),
            templates: Vec::new(),
            inherits: Vec::new(),
            attributes: Vec::new(),
            docblock: None,
            builtin: Some(meta(record)),
        };
        for member in self.data.members(&record.name, self.version) {
            converter.member(&mut decl, member, &context);
        }
        decl
    }
}

fn available(record: &Record) -> Availability {
    Availability::Available {
        extension: record.extension.as_str().into(),
        guards: record.guards.iter().map(|g| g.as_str().into()).collect(),
        deprecated: record.deprecation.is_some(),
    }
}

fn meta(record: &Record) -> BuiltinMeta {
    BuiltinMeta {
        extension: record.extension.as_str().into(),
        guards: record.guards.iter().map(|g| g.as_str().into()).collect(),
        deprecated: record.deprecation.is_some(),
    }
}

fn syntax_kind(kind: ClassKind) -> ClassLikeKind {
    match kind {
        ClassKind::Interface => ClassLikeKind::Interface,
        ClassKind::Trait => ClassLikeKind::Trait,
        ClassKind::Enum => ClassLikeKind::Enum,
        _ => ClassLikeKind::Class,
    }
}

fn visibility(v: DataVisibility) -> Visibility {
    match v {
        DataVisibility::Protected => Visibility::Protected,
        DataVisibility::Private => Visibility::Private,
        _ => Visibility::Public,
    }
}

struct Converter {
    diagnostics: Vec<SymbolDiagnostic>,
}

impl Converter {
    /// Converts type text. Unparseable text is reported and becomes `unknown`.
    fn convert(&mut self, text: &str, symbol: &str, ctx: &ClassContext) -> Option<Type> {
        if text.trim().is_empty() {
            return None;
        }
        let type_ctx = TypeContext::new().with_class(ctx);
        if let Some(ty) = parse_type(text, &type_ctx, 0).ty {
            return Some(ty);
        }
        self.diagnostics.push(SymbolDiagnostic::new(
            SymbolDiagnosticCode::BuiltinData,
            format!("cannot read the type `{text}` of `{symbol}`"),
            Span::new(0, 0),
        ));
        Some(Type::unknown())
    }

    fn native(&mut self, text: &str, symbol: &str) -> Type {
        self.convert(text, symbol, &ClassContext::none())
            .unwrap_or_else(Type::unknown)
    }

    /// The declared type, else the tentative one, refined by the docblock type.
    fn effective(
        &mut self,
        [declared, tentative, doc]: [&str; 3],
        symbol: &str,
        ctx: &ClassContext,
    ) -> EffectiveType {
        let native_text = if declared.is_empty() {
            tentative
        } else {
            declared
        };
        let native = self.convert(native_text, symbol, ctx);
        let doc = self.convert(doc, symbol, ctx);
        let mut found = Vec::new();
        let merged = merge_types(native, doc, Span::new(0, 0), &mut found);
        // Class names in built-in docblocks cannot be checked against a hierarchy yet, so the
        // unverified note would be noise for thousands of records; contradictions are kept.
        self.diagnostics.extend(
            found
                .into_iter()
                .filter(|d| d.code != SymbolDiagnosticCode::DocblockUnverified),
        );
        merged
    }

    fn params(&mut self, params: &[Param], ctx: &ClassContext, symbol: &str) -> Vec<ParamDecl> {
        params
            .iter()
            .map(|p| ParamDecl {
                name: p.name.as_str().into(),
                ty: self.effective([&p.type_text, "", &p.doc_type], symbol, ctx),
                by_ref: p.by_ref,
                variadic: p.variadic,
                has_default: p.optional || p.default.is_some(),
                attributes: Vec::new(),
            })
            .collect()
    }

    fn signature(
        &mut self,
        signature: &Signature,
        ctx: &ClassContext,
        symbol: &str,
    ) -> (Vec<ParamDecl>, EffectiveType, bool) {
        let params = self.params(&signature.params, ctx, symbol);
        let ret = self.effective(
            [
                &signature.return_type,
                &signature.tentative_return_type,
                &signature.doc_return_type,
            ],
            symbol,
            ctx,
        );
        (params, ret, signature.return_by_ref)
    }

    fn member(&mut self, decl: &mut ClassDecl, record: &Record, ctx: &ClassContext) {
        let short = record
            .name
            .split_once("::")
            .map_or(record.name.as_str(), |(_, m)| m);
        match &record.detail {
            Detail::Method(m) => decl.methods.push(self.method(short, m, record, ctx)),
            Detail::Property(p) => decl.properties.push(PropertyDecl {
                name: short.into(),
                span: Span::new(0, 0),
                visibility: visibility(p.visibility),
                set_visibility: None,
                is_static: p.is_static,
                is_readonly: p.is_readonly,
                ty: self.effective([&p.type_text, "", &p.doc_type], &record.name, ctx),
                has_default: p.default.is_some(),
                hooks: Vec::new(),
                origin: MemberOrigin::Declared,
                attributes: Vec::new(),
            }),
            Detail::ClassConstant(c) => decl.constants.push(ConstDecl {
                name: short.into(),
                span: Span::new(0, 0),
                visibility: visibility(c.visibility),
                is_final: c.is_final,
                ty: self
                    .convert(&c.type_text, &record.name, ctx)
                    .unwrap_or_else(Type::unknown),
            }),
            Detail::EnumCase(_) => decl.cases.push(CaseDecl {
                name: short.into(),
                span: Span::new(0, 0),
                value: None,
            }),
            _ => {}
        }
    }

    fn method(
        &mut self,
        short: &str,
        detail: &MethodDetail,
        record: &Record,
        ctx: &ClassContext,
    ) -> MethodDecl {
        let (params, ret, by_ref) = self.signature(&detail.signature, ctx, &record.name);
        MethodDecl {
            name: short.into(),
            span: Span::new(0, 0),
            visibility: visibility(detail.visibility),
            is_static: detail.is_static,
            is_final: detail.is_final,
            is_abstract: detail.is_abstract,
            params,
            ret,
            by_ref,
            origin: MemberOrigin::Declared,
            attributes: Vec::new(),
            docblock: None,
            templates: Vec::new(),
        }
    }
}
