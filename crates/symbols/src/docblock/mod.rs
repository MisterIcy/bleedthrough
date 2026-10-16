//! Docblock parsing: the type expressions of the supported PHPDoc and Psalm subset and the tags
//! that carry them.

mod keywords;
mod lexer;
mod split;
mod tags;
mod typeparse;

pub use tags::{
    DocBlock, DocMethod, DocMethodParam, DocParam, DocProperty, DocReturn, DocTemplate, DocVar,
    PropertyAccess, parse_docblock,
};
pub use typeparse::{TypeContext, TypeParse, parse_type};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod robustness;
