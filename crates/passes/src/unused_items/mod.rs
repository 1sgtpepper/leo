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

//! Emits warnings for items that are never used. Coverage mirrors `rustc`'s
//! `dead_code` and `unused_variables` lints:
//!
//! - Unused functions (any non-entry-point, non-test variant) — uses the
//!   holistic `call_count`. A leading `_` on the function name silences the
//!   warning (and, for top-level `Variant::Fn`, also forces inlining so the
//!   name never reaches the VM as a closure identifier).
//! - Unused structs — uses a local reachability graph built from composite
//!   members (records and user-referenced composites are roots).
//! - Unused `const` declarations — at every level (program scope, modules,
//!   library, function bodies). Consts are fully inlined at compile time and
//!   never reach the VM, so a `_X` prefix safely silences the warning.
//! - Unused local bindings — `let`, function/`final fn` parameters, and loop
//!   iteration variables that are never read.
//! - Unused imports — `import program.aleo;` declarations whose program is
//!   never referenced in a path.
//!
//! The pass runs in three phases, all driven by the standard `AstVisitor` /
//! `UnitVisitor` traits:
//!
//! 1. [`collect::UseCollector`] walks the entire AST. It populates
//!    `used_imports` and `used_globals`; tracks lexical scopes and emits
//!    unused-local warnings as each scope drains; and records each
//!    composite's outgoing member-type edges into a local dependency graph.
//! 2. A pure reachability scan over that dependency graph computes the set of
//!    composites transitively reachable from "live roots" (records + any
//!    composite directly referenced from non-struct-definition code).
//! 3. [`check::UnusedChecker`] walks the AST again, this time without
//!    descending into function bodies. Its overrides of `visit_function`,
//!    `visit_composite`, `visit_const`, and `visit_program` emit warnings
//!    for top-level items and imports that the collected data flagged as
//!    unused.
//!
//! Dead struct fields are tracked-but-not-warned: today the analysis is purely
//! syntactic (a field is "read" only when accessed via `MemberAccess`), which
//! produces too much noise on pass-through-only structs. A future refinement
//! (likely a data-flow-aware variant, plus an `@allow_unused` attribute) will
//! re-introduce that warning. Unreachable code after `return` is not handled
//! here — the type checker already rejects it with `ETYC0372025`.
//!
//! Only the compilation root (the user's own `Program` or `Library`) is
//! analyzed; imported programs/libraries are skipped because the same
//! dependency may be consumed differently by other compilations.

mod check;
mod collect;

use crate::Pass;

use leo_ast::*;
use leo_errors::Result;
use leo_span::Symbol;

use indexmap::{IndexMap, IndexSet};

pub struct UnusedItems;

impl Pass for UnusedItems {
    type Input = ();
    type Output = ();

    const NAME: &str = "UnusedItems";

    fn do_pass(_input: Self::Input, state: &mut crate::CompilerState) -> Result<Self::Output> {
        let ast = std::mem::take(&mut state.ast);

        // Phase 1 — `UseCollector` walks the AST collecting references and emitting
        // unused-local warnings as each scope drains.
        let collected = {
            let mut collector = collect::UseCollector::new(state);
            match &ast {
                Ast::Program(program) => collector.visit_program(program),
                Ast::Library(library) => collector.visit_library(library),
            }
            collector.into_data()
        };

        // Phase 2 — reachability over the composite member-dependency edges captured by
        // the collector. Roots are records + any composite the user code referenced
        // outside of another composite's member type.
        let live_composites = collected.compute_live_composites();

        // Phase 3 — `UnusedChecker` walks the AST again (without descending into bodies)
        // and emits warnings for top-level items and imports that the collected data
        // flagged as unused.
        {
            let mut checker = check::UnusedChecker::new(state, &collected, &live_composites);
            match &ast {
                Ast::Program(program) => checker.visit_program(program),
                Ast::Library(library) => checker.visit_library(library),
            }
        }

        // Restore the AST before propagating any error so downstream passes don't see
        // `Ast::default()` if a future refinement starts emitting errors from this pass.
        state.ast = ast;
        state.handler.last_err()?;
        Ok(())
    }
}

/// True if the interned symbol's text starts with `_`. Inspects the underlying string
/// slice via the session globals so we don't allocate a `String` just to look at the
/// first byte; matches the pattern used elsewhere (e.g. `Path::is_aleo_program`).
pub(crate) fn name_starts_with_underscore(name: Symbol) -> bool {
    leo_span::with_session_globals(|sg| name.as_str(sg, |s| s.starts_with('_')))
}

/// Owned data produced by [`collect::UseCollector`] and consumed by the reachability
/// scan and [`check::UnusedChecker`].
pub(super) struct CollectedUses {
    pub(super) used_imports: IndexSet<Symbol>,
    pub(super) used_globals: IndexSet<Location>,
    /// Member-type edges: `composite_deps[A]` are the composites directly referenced
    /// by `A`'s member types. Records and structs alike contribute entries here.
    pub(super) composite_deps: IndexMap<Location, IndexSet<Location>>,
    /// Composites that are always live regardless of user references: records (program
    /// surface) and library top-level structs (public surface).
    pub(super) composite_roots: Vec<Location>,
}

impl CollectedUses {
    /// Forward BFS in the composite-dependency graph from the live-root set, which
    /// combines records / library top-level structs (`composite_roots`) with any
    /// composite directly referenced from user code (`used_globals`).
    pub(super) fn compute_live_composites(&self) -> IndexSet<Location> {
        let mut live: IndexSet<Location> = IndexSet::new();
        let mut queue: Vec<Location> = Vec::with_capacity(self.composite_roots.len() + self.used_globals.len());
        queue.extend(self.composite_roots.iter().cloned());
        queue.extend(self.used_globals.iter().cloned());
        while let Some(loc) = queue.pop() {
            if !live.insert(loc.clone()) {
                continue;
            }
            if let Some(children) = self.composite_deps.get(&loc) {
                for child in children {
                    queue.push(child.clone());
                }
            }
        }
        live
    }
}
