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

//! Phase 3 of the unused-items pass: walks the AST top-down emitting warnings
//! for unused top-level items and imports. Function bodies are not visited —
//! the body-level warnings were already emitted by the collector in phase 1.

use super::{CollectedUses, name_starts_with_underscore};

use leo_ast::*;
use leo_span::{Symbol, sym};

use indexmap::IndexSet;

pub(super) struct UnusedChecker<'a, 'd> {
    state: &'a mut crate::CompilerState,
    data: &'d CollectedUses,
    live_composites: &'d IndexSet<Location>,
    /// Current compilation unit while walking. Pushed/popped by `visit_program_scope`,
    /// `visit_module`, and `visit_library`.
    unit: Symbol,
    /// Current module path prefix (empty at program/library top level).
    prefix: Vec<Symbol>,
}

impl<'a, 'd> UnusedChecker<'a, 'd> {
    pub(super) fn new(
        state: &'a mut crate::CompilerState,
        data: &'d CollectedUses,
        live_composites: &'d IndexSet<Location>,
    ) -> Self {
        Self { state, data, live_composites, unit: Symbol::intern(""), prefix: Vec::new() }
    }

    fn current_location(&self, name: Symbol) -> Location {
        Location::new(self.unit, self.prefix.iter().copied().chain(std::iter::once(name)).collect())
    }
}

impl AstVisitor for UnusedChecker<'_, '_> {
    type AdditionalInput = ();
    type Output = ();

    fn visit_const(&mut self, input: &ConstDeclaration) {
        // Reached only from `visit_program_scope` / `visit_module` (the checker never
        // descends into function bodies), so every `input` here is a top-level / module-
        // scope const.
        if name_starts_with_underscore(input.place.name) {
            return;
        }
        let location = self.current_location(input.place.name);
        if self.data.used_globals.contains(&location) {
            return;
        }
        self.state.handler.emit_warning(crate::errors::unused_items::unused_const(input.place.name, input.place.span));
    }
}

impl UnitVisitor for UnusedChecker<'_, '_> {
    fn visit_program(&mut self, input: &Program) {
        for scope in input.program_scopes.values() {
            self.visit_program_scope(scope);
        }
        for module in input.modules.values() {
            self.visit_module(module);
        }
        for (import_name, program_id) in &input.imports {
            if self.data.used_imports.contains(import_name) {
                continue;
            }
            self.state.handler.emit_warning(crate::errors::unused_items::unused_import(import_name, program_id.span()));
        }
    }

    fn visit_library(&mut self, input: &Library) {
        // Top-level fns, consts, and structs are the library's public surface; consumers
        // calling/reading them live in other compilation units and don't show up in our
        // local counts. Warning on them produces spurious noise on every `leo build` of a
        // library, so skip the top-level set entirely. Submodule items are not public
        // surface (no re-export today) and are still checked.
        let prev_unit = std::mem::replace(&mut self.unit, input.name);
        let prev_prefix = std::mem::take(&mut self.prefix);
        for module in input.modules.values() {
            self.visit_module(module);
        }
        self.unit = prev_unit;
        self.prefix = prev_prefix;
    }

    fn visit_program_scope(&mut self, input: &ProgramScope) {
        let prev_unit = std::mem::replace(&mut self.unit, input.program_id.as_symbol());
        let prev_prefix = std::mem::take(&mut self.prefix);
        // Iterate functions → composites → consts (rather than the default
        // consts → composites → functions order) so the warning output groups by item
        // kind in the same order the original direct-iteration code produced.
        for (_, f) in &input.functions {
            self.visit_function(f);
        }
        for (_, c) in &input.composites {
            self.visit_composite(c);
        }
        for (_, c) in &input.consts {
            self.visit_const(c);
        }
        self.unit = prev_unit;
        self.prefix = prev_prefix;
    }

    fn visit_module(&mut self, input: &Module) {
        let prev_unit = std::mem::replace(&mut self.unit, input.unit_name);
        let prev_prefix = std::mem::replace(&mut self.prefix, input.path.clone());
        for (_, f) in &input.functions {
            self.visit_function(f);
        }
        for (_, c) in &input.composites {
            self.visit_composite(c);
        }
        for (_, c) in &input.consts {
            self.visit_const(c);
        }
        self.unit = prev_unit;
        self.prefix = prev_prefix;
    }

    fn visit_function(&mut self, input: &Function) {
        // Roots are always live: entry points and `@test` functions. `Variant::Finalize` is
        // produced by a later pass (`ProcessingAsync`) and cannot appear here.
        if input.variant.is_entry() {
            return;
        }
        if input.annotations.iter().any(|a| a.identifier.name == sym::test) {
            return;
        }
        // A leading `_` on the function name signals intentionally-unused, mirroring `rustc`'s
        // `_x` convention. Safe everywhere we warn: `Variant::FinalFn` is always inlined, and
        // `Variant::Fn` with a `_` prefix is force-inlined by `function_inlining`, so neither
        // ever reaches the VM as a named identifier.
        if name_starts_with_underscore(input.identifier.name) {
            return;
        }
        let location = self.current_location(input.identifier.name);
        let call_count =
            *self.state.call_count.get(&location).expect("call_count is populated for every function by TypeChecking");
        if call_count > 0 {
            return;
        }
        self.state
            .handler
            .emit_warning(crate::errors::unused_items::unused_function(input.identifier.name, input.identifier.span));
    }

    fn visit_composite(&mut self, input: &Composite) {
        // Records are part of the program's public surface; their shape is constrained by
        // interface conformance, so we don't warn on them.
        if input.is_record {
            return;
        }
        let location = self.current_location(input.identifier.name);
        if !self.live_composites.contains(&location) {
            self.state
                .handler
                .emit_warning(crate::errors::unused_items::unused_struct(input.identifier.name, input.identifier.span));
        }
        // Dead struct fields are intentionally not warned: see the module docstring.
    }

    // The checker only emits warnings for top-level items, so suppress descents into
    // body-bearing or non-warning items. (The default impls would recurse into
    // function-prototype types, mapping types, etc., which the checker doesn't care about.)
    fn visit_constructor(&mut self, _input: &Constructor) {}

    fn visit_interface(&mut self, _input: &Interface) {}

    fn visit_mapping(&mut self, _input: &Mapping) {}

    fn visit_storage_variable(&mut self, _input: &StorageVariable) {}

    fn visit_stub(&mut self, _input: &Stub) {}

    fn visit_function_stub(&mut self, _input: &leo_ast::FunctionStub) {}

    fn visit_composite_stub(&mut self, _input: &Composite) {}
}
