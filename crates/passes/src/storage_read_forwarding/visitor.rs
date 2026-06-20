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

use crate::CompilerState;

use leo_ast::{Expression, IntrinsicExpression, LiteralVariant};
use leo_span::{Symbol, sym};

use indexmap::IndexMap;

#[derive(Clone, Eq, PartialEq, Hash)]
pub(super) enum Atom {
    Path(Vec<Symbol>),
    Literal(LiteralVariant),
}

#[derive(Eq, PartialEq, Hash)]
pub(super) enum StorageRead {
    Get { mapping: Atom, key: Atom },
    GetOrUse { mapping: Atom, key: Atom, default: Atom },
    Contains { mapping: Atom, key: Atom },
}

pub struct StorageReadForwardingVisitor<'a> {
    pub state: &'a mut CompilerState,
    pub(super) reads: IndexMap<StorageRead, Symbol>,
    pub(super) aliases: IndexMap<Symbol, Symbol>,
}

impl StorageReadForwardingVisitor<'_> {
    pub(super) fn clear_reads(&mut self) {
        self.reads.clear();
    }

    pub(super) fn clear_function_state(&mut self) {
        self.reads.clear();
        self.aliases.clear();
    }

    pub(super) fn local_alias(&self, name: Symbol) -> Option<Symbol> {
        let mut current = name;
        while let Some(next) = self.aliases.get(&current).copied() {
            if next == current {
                return Some(current);
            }
            current = next;
        }
        (current != name).then_some(current)
    }

    pub(super) fn atom(expr: &Expression) -> Option<Atom> {
        match expr {
            Expression::Literal(lit) => Some(Atom::Literal(lit.variant.clone())),
            Expression::Path(path) => {
                Some(Atom::Path(path.qualifier().iter().map(|id| id.name).chain([path.identifier().name]).collect()))
            }
            _ => None,
        }
    }

    pub(super) fn storage_read(intrinsic: &IntrinsicExpression) -> Option<StorageRead> {
        match intrinsic.name {
            sym::_mapping_get => Some(StorageRead::Get {
                mapping: Self::atom(intrinsic.arguments.first()?)?,
                key: Self::atom(intrinsic.arguments.get(1)?)?,
            }),
            sym::_mapping_get_or_use => Some(StorageRead::GetOrUse {
                mapping: Self::atom(intrinsic.arguments.first()?)?,
                key: Self::atom(intrinsic.arguments.get(1)?)?,
                default: Self::atom(intrinsic.arguments.get(2)?)?,
            }),
            sym::_mapping_contains => Some(StorageRead::Contains {
                mapping: Self::atom(intrinsic.arguments.first()?)?,
                key: Self::atom(intrinsic.arguments.get(1)?)?,
            }),
            _ => None,
        }
    }

    pub(super) fn is_effect_boundary(intrinsic: &IntrinsicExpression) -> bool {
        matches!(intrinsic.name, sym::_mapping_set | sym::_mapping_remove | sym::_final_run)
    }
}
