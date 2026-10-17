//! Index newtypes of a graph. Each is dense and assigned in source order.

use std::fmt;

macro_rules! index_type {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// Builds an id from its index.
            pub const fn new(index: u32) -> Self {
                Self(index)
            }

            /// The index.
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            /// The raw number.
            pub const fn raw(self) -> u32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

index_type!(
    /// Term: exempt
    ///
    /// A single-assignment temporary: defined by exactly one instruction.
    ValueId,
    "v"
);
index_type!(
    /// Term: exempt
    ///
    /// A basic block of a graph.
    BlockId,
    "b"
);
index_type!(
    /// Term: exempt
    ///
    /// A mutable local variable of a unit, interned by name in order of first appearance.
    LocalId,
    "l"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_display_with_their_prefix() {
        assert_eq!(ValueId::new(3).to_string(), "v3");
        assert_eq!(BlockId::new(0).to_string(), "b0");
        assert_eq!(LocalId::new(12).to_string(), "l12");
        assert_eq!(BlockId::new(7).index(), 7);
        assert_eq!(LocalId::new(7).raw(), 7);
    }
}
