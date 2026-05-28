// Copyright (C) 2019-2026 Provable Inc.
// This file is part of the Leo library.

// The Leo library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The Leo library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the Leo library. If not, see <https://www.gnu.org/licenses/>.

//! Warnings for unused items. Wording mirrors `rustc`'s `dead_code`,
//! `unused_variables`, `unused_imports`, and `unreachable_code` lints.

use leo_errors::Formatted;
use leo_span::Span;
use std::fmt::Display;

const CODE_PREFIX: &str = "UNU";
const CODE_MASK: i32 = 14000;

// Warnings

/// `dead_code`: "function `foo` is never used".
pub(crate) fn unused_function(name: impl Display, span: Span) -> Formatted {
    Formatted::warning(CODE_PREFIX, CODE_MASK, format!("function `{name}` is never used"), span)
}

/// `unused_variables`: "unused variable: `x`".
pub(crate) fn unused_variable(name: impl Display, span: Span) -> Formatted {
    Formatted::warning(CODE_PREFIX, CODE_MASK + 1, format!("unused variable: `{name}`"), span)
}

/// `dead_code` on a struct: "struct `Foo` is never constructed".
pub(crate) fn unused_struct(name: impl Display, span: Span) -> Formatted {
    Formatted::warning(CODE_PREFIX, CODE_MASK + 2, format!("struct `{name}` is never constructed"), span)
}

/// `unused_imports`: "unused import: `program.aleo`".
pub(crate) fn unused_import(name: impl Display, span: Span) -> Formatted {
    Formatted::warning(CODE_PREFIX, CODE_MASK + 4, format!("unused import: `{name}`"), span)
}

/// `dead_code` on a const: "constant `X` is never used".
pub(crate) fn unused_const(name: impl Display, span: Span) -> Formatted {
    Formatted::warning(CODE_PREFIX, CODE_MASK + 5, format!("constant `{name}` is never used"), span)
}

/// `used_underscore_binding`: a leading-`_` binding was actually read, defeating the
/// "intentionally unused" marker.
pub(crate) fn used_underscore_binding(name: impl Display, span: Span) -> Formatted {
    Formatted::warning(CODE_PREFIX, CODE_MASK + 6, format!("used binding `{name}` whose name begins with `_`"), span)
        .with_help("Remove the leading `_` from the name, or stop reading the binding.")
}
