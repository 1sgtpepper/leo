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

//! Phase 1 of the unused-items pass: walks the AST collecting reference
//! information (used imports, used globals, composite-dependency edges) and
//! emits unused-local warnings as each lexical scope drains.

use super::{CollectedUses, name_starts_with_underscore};

use leo_ast::*;
use leo_span::{Span, Symbol, sym};

use indexmap::{IndexMap, IndexSet};

#[derive(Copy, Clone)]
enum BindingKind {
    /// A `let` binding, function/iter-var parameter, or `for` iteration variable.
    Variable,
    /// A local `const` declaration. Emits `constant X is never used` instead of the
    /// `unused variable` text used for the other kinds.
    Const,
}

struct Binding {
    name: Symbol,
    span: Span,
    kind: BindingKind,
    referenced: bool,
}

pub(super) struct UseCollector<'a> {
    state: &'a mut crate::CompilerState,
    /// Locals currently in scope, in declaration order. `exit_scope` drains the tail —
    /// emitting unused-warnings for everything popped — so a per-name shadowing index
    /// isn't needed: a reference simply scans from the end for the innermost match.
    bindings: Vec<Binding>,
    /// `bindings.len()` snapshot at each scope entry; `exit_scope` truncates back to it.
    scope_starts: Vec<usize>,
    used_imports: IndexSet<Symbol>,
    used_globals: IndexSet<Location>,
    composite_deps: IndexMap<Location, IndexSet<Location>>,
    composite_roots: Vec<Location>,
    /// Current compilation unit while walking. Pushed/popped by `visit_program_scope`,
    /// `visit_module`, and `visit_library`.
    unit: Symbol,
    /// Current module path prefix (empty at program/library top level).
    prefix: Vec<Symbol>,
}

impl<'a> UseCollector<'a> {
    pub(super) fn new(state: &'a mut crate::CompilerState) -> Self {
        Self {
            state,
            bindings: Vec::new(),
            scope_starts: Vec::new(),
            used_imports: IndexSet::new(),
            used_globals: IndexSet::new(),
            composite_deps: IndexMap::new(),
            composite_roots: Vec::new(),
            unit: Symbol::intern(""),
            prefix: Vec::new(),
        }
    }

    pub(super) fn into_data(self) -> CollectedUses {
        CollectedUses {
            used_imports: self.used_imports,
            used_globals: self.used_globals,
            composite_deps: self.composite_deps,
            composite_roots: self.composite_roots,
        }
    }

    fn current_location(&self, name: Symbol) -> Location {
        Location::new(self.unit, self.prefix.iter().copied().chain(std::iter::once(name)).collect())
    }

    fn declare(&mut self, name: Symbol, span: Span, kind: BindingKind) {
        self.bindings.push(Binding { name, span, kind, referenced: false });
    }

    fn enter_scope(&mut self) {
        self.scope_starts.push(self.bindings.len());
    }

    /// Drains every binding that was introduced inside this scope, emitting unused-warnings
    /// for those that were never read (and aren't `_`-prefixed).
    fn exit_scope(&mut self) {
        let start = self.scope_starts.pop().expect("enter/exit_scope must balance");
        for b in self.bindings.drain(start..) {
            // A leading `_` signals intentionally-unused, matching `rustc`'s `_x` convention.
            // Safe to silence locally — local names never reach the VM (they're SSA-renamed
            // or substituted out before code generation).
            if b.referenced || name_starts_with_underscore(b.name) {
                continue;
            }
            let warning = match b.kind {
                BindingKind::Variable => crate::errors::unused_items::unused_variable(b.name, b.span),
                BindingKind::Const => crate::errors::unused_items::unused_const(b.name, b.span),
            };
            self.state.handler.emit_warning(warning);
        }
    }

    /// Mark the innermost in-scope binding for `name` as referenced. Returns `true` if a
    /// binding matched, `false` otherwise (the path resolved to a global, handled via
    /// `used_globals`, or to nothing, in which case earlier passes have already errored).
    fn note_local_use(&mut self, name: Symbol) -> bool {
        // Two-step extraction: read what we need from the `&mut Binding`, then drop the
        // borrow before reborrowing `self.state` to emit the warning.
        let mut warn_underscore: Option<(Symbol, Span)> = None;
        let matched = if let Some(b) = self.bindings.iter_mut().rev().find(|b| b.name == name) {
            // Reading a `_`-prefixed binding defeats the silencing marker; warn once on the
            // first read (subsequent reads see `referenced == true` and stay silent).
            if !b.referenced && name_starts_with_underscore(b.name) {
                warn_underscore = Some((b.name, b.span));
            }
            b.referenced = true;
            true
        } else {
            false
        };
        if let Some((name, span)) = warn_underscore {
            self.state.handler.emit_warning(crate::errors::unused_items::used_underscore_binding(name, span));
        }
        matched
    }

    /// Whether this function's parameters should be checked. `Variant::Finalize` is produced
    /// by a later pass (`ProcessingAsync`) and cannot appear here.
    fn track_parameters(function: &Function) -> bool {
        !function.variant.is_entry() && !function.annotations.iter().any(|a| a.identifier.name == sym::test)
    }

    /// Track a path's contribution to import usage and local references. Called from
    /// `visit_path` (expression paths) and from the explicit overrides for paths that the
    /// default walk skips (call function paths, composite-init paths, composite-type paths).
    fn note_path(&mut self, path: &Path) {
        if let Some(pid) = path.user_program() {
            self.used_imports.insert(pid.as_symbol());
        }
        if path.is_local() {
            self.note_local_use(path.identifier().name);
        }
        if let Some(loc) = path.try_global_location() {
            self.used_globals.insert(loc.clone());
        }
        if !path.is_resolved() {
            // Defensive: a path that resolved to nothing is either an intrinsic (e.g.
            // `_dynamic_call`) or an unresolved name. Earlier passes will have already errored
            // on the latter; to stay robust against future error-recovery work, treat the bare
            // identifier as a potential local reference.
            self.note_local_use(path.identifier().name);
        }
    }

    /// Walk the LHS of an assignment for read-effects only: the assignment root (the path
    /// or outermost field/tuple/array access) is a write and must not be treated as a use.
    /// Any indices and the navigation into the target are reads. Mirrors
    /// `cei_analysis::visit_assign_lhs_reads` — both skip the root `Path` so that a
    /// write-only-never-read local is correctly flagged as unused (matching `rustc`'s
    /// `unused_variables` lint, which fires on `let mut x = 0; x = 5;`).
    fn walk_assign_place(&mut self, expr: &Expression) {
        match expr {
            // Root path is a write target, not a use.
            Expression::Path(_) => {}
            // Outermost field is the write target; recurse into the inner navigation.
            Expression::MemberAccess(access) => self.walk_assign_place(&access.inner),
            Expression::TupleAccess(access) => self.walk_assign_place(&access.tuple),
            Expression::ArrayAccess(access) => {
                self.walk_assign_place(&access.array);
                self.visit_expression(&access.index, &Default::default());
            }
            // Any other shape on the LHS shouldn't reach this stage (type checker rejects),
            // but fall back to a normal read just in case.
            other => self.visit_expression(other, &Default::default()),
        }
    }

    /// Record the composite-dependency edges (`loc → composites referenced in members`)
    /// for `loc`'s composite. Records / library top-level structs additionally seed the
    /// reachability roots via `mark_root`.
    fn record_composite_member_deps(&mut self, loc: Location, composite: &Composite) {
        let mut refs = IndexSet::new();
        for member in &composite.members {
            collect_type_composite_refs(&member.type_, &mut refs);
        }
        self.composite_deps.insert(loc, refs);
    }
}

impl AstVisitor for UseCollector<'_> {
    type AdditionalInput = ();
    type Output = ();

    fn visit_path(&mut self, input: &Path, _additional: &Self::AdditionalInput) -> Self::Output {
        self.note_path(input);
    }

    fn visit_block(&mut self, input: &Block) {
        // Blocks are the scope boundary for `let` / local `const` / iter-var shadowing, so a
        // later same-name reference cannot retroactively silence an earlier unused binding.
        self.enter_scope();
        for stmt in &input.statements {
            self.visit_statement(stmt);
        }
        self.exit_scope();
    }

    fn visit_definition(&mut self, input: &DefinitionStatement) {
        // Visit the RHS first so a `let x = x + 1` style shadow resolves the RHS `x` to the
        // outer binding rather than to itself.
        if let Some(ty) = input.type_.as_ref() {
            self.visit_type(ty);
        }
        self.visit_expression(&input.value, &Default::default());
        match &input.place {
            DefinitionPlace::Single(id) => {
                self.declare(id.name, id.span, BindingKind::Variable);
            }
            DefinitionPlace::Multiple(ids) => {
                for id in ids {
                    self.declare(id.name, id.span, BindingKind::Variable);
                }
            }
        }
    }

    fn visit_const(&mut self, input: &ConstDeclaration) {
        self.visit_type(&input.type_);
        self.visit_expression(&input.value, &Default::default());
        // Only track local `const`s as bindings. Top-level/module-scope `const`s reach this
        // method via the default `visit_program_scope`/`visit_module` walks (no enclosing
        // scope is open); they are checked separately via `used_globals` in the checker, so
        // pushing them into `bindings` here would pollute `note_local_use`'s shadowing scan.
        if !self.scope_starts.is_empty() {
            self.declare(input.place.name, input.place.span, BindingKind::Const);
        }
    }

    fn visit_iteration(&mut self, input: &IterationStatement) {
        if let Some(ty) = input.type_.as_ref() {
            self.visit_type(ty);
        }
        self.visit_expression(&input.start, &Default::default());
        self.visit_expression(&input.stop, &Default::default());
        // Iter var is in scope only during the loop body.
        self.enter_scope();
        self.declare(input.variable.name, input.variable.span, BindingKind::Variable);
        self.visit_block(&input.block);
        self.exit_scope();
    }

    fn visit_assign(&mut self, input: &AssignStatement) {
        self.walk_assign_place(&input.place);
        self.visit_expression(&input.value, &Default::default());
    }

    fn visit_call(&mut self, input: &CallExpression, _additional: &Self::AdditionalInput) -> Self::Output {
        self.note_path(&input.function);
        for expr in &input.const_arguments {
            self.visit_expression(expr, &Default::default());
        }
        for expr in &input.arguments {
            self.visit_expression(expr, &Default::default());
        }
    }

    fn visit_composite_init(
        &mut self,
        input: &CompositeExpression,
        _additional: &Self::AdditionalInput,
    ) -> Self::Output {
        self.note_path(&input.path);
        for expr in &input.const_arguments {
            self.visit_expression(expr, &Default::default());
        }
        for member in &input.members {
            // After `PathResolution::reconstruct_composite_init`, every resolvable shorthand
            // `Foo { a }` has been desugared into `Foo { a: <resolved path>}`. Walking the
            // expression then routes through `visit_path` → `note_path` exactly as for an
            // explicit `a: a`. Shorthand entries that remain `None` reach here only when the
            // identifier did not resolve at all — the type checker emits the focused error
            // in that case, so there is nothing more for us to do.
            if let Some(expression) = &member.expression {
                self.visit_expression(expression, &Default::default());
            }
        }
    }

    fn visit_composite_type(&mut self, input: &CompositeType) {
        self.note_path(&input.path);
        for expr in &input.const_arguments {
            self.visit_expression(expr, &Default::default());
        }
    }
}

impl UnitVisitor for UseCollector<'_> {
    fn visit_program(&mut self, input: &Program) {
        for scope in input.program_scopes.values() {
            self.visit_program_scope(scope);
        }
        for module in input.modules.values() {
            self.visit_module(module);
        }
        // `stubs` (imported programs) are not user code; skip.
    }

    fn visit_library(&mut self, input: &Library) {
        let prev_unit = std::mem::replace(&mut self.unit, input.name);
        let prev_prefix = std::mem::take(&mut self.prefix);

        for (_, c) in &input.consts {
            self.visit_const(c);
        }
        // Top-level library structs are public surface — always live roots, regardless of
        // whether anything in this compilation unit references them.
        for (name, composite) in &input.structs {
            self.composite_roots.push(Location::new(self.unit, vec![*name]));
            self.visit_composite(composite);
        }
        for (_, f) in &input.functions {
            self.visit_function(f);
        }
        for (_, i) in &input.interfaces {
            self.visit_interface(i);
        }
        for module in input.modules.values() {
            self.visit_module(module);
        }
        // `stubs` are not user code; skip.

        self.unit = prev_unit;
        self.prefix = prev_prefix;
    }

    fn visit_program_scope(&mut self, input: &ProgramScope) {
        let prev_unit = std::mem::replace(&mut self.unit, input.program_id.as_symbol());
        let prev_prefix = std::mem::take(&mut self.prefix);

        for (_, c) in &input.consts {
            self.visit_const(c);
        }
        for (_, c) in &input.composites {
            self.visit_composite(c);
        }
        for (_, i) in &input.interfaces {
            self.visit_interface(i);
        }
        for (_, m) in &input.mappings {
            self.visit_mapping(m);
        }
        for (_, s) in &input.storage_variables {
            self.visit_storage_variable(s);
        }
        for (_, f) in &input.functions {
            self.visit_function(f);
        }
        if let Some(c) = input.constructor.as_ref() {
            self.visit_constructor(c);
        }

        self.unit = prev_unit;
        self.prefix = prev_prefix;
    }

    fn visit_module(&mut self, input: &Module) {
        let prev_unit = std::mem::replace(&mut self.unit, input.unit_name);
        let prev_prefix = std::mem::replace(&mut self.prefix, input.path.clone());

        for (_, c) in &input.consts {
            self.visit_const(c);
        }
        for (_, c) in &input.composites {
            self.visit_composite(c);
        }
        for (_, i) in &input.interfaces {
            self.visit_interface(i);
        }
        for (_, f) in &input.functions {
            self.visit_function(f);
        }

        self.unit = prev_unit;
        self.prefix = prev_prefix;
    }

    fn visit_composite(&mut self, input: &Composite) {
        let loc = self.current_location(input.identifier.name);
        // Const_parameters' types are restricted to primitives but visiting them keeps the
        // walk uniform and is cheap.
        for cp in &input.const_parameters {
            self.visit_type(&cp.type_);
        }
        // Record the structural member edges for the reachability scan in `compute_live_composites`.
        self.record_composite_member_deps(loc.clone(), input);
        // Records are always live program surface — their members ARE user-driven (record
        // fields reach the VM), so walk member types so referenced composites land in
        // `used_globals` too. Struct members are intentionally skipped here: visiting them
        // would let a transitively-dead struct keep its inner composites alive.
        if input.is_record {
            self.composite_roots.push(loc);
            for member in &input.members {
                self.visit_type(&member.type_);
            }
        }
    }

    fn visit_function(&mut self, input: &Function) {
        // Function-level scope holds the params (in scope for the whole body). The body's
        // own block then opens a nested scope. On exit, both scopes drain and emit warnings.
        self.enter_scope();
        if Self::track_parameters(input) {
            for cp in &input.const_parameters {
                self.declare(cp.identifier.name, cp.identifier.span, BindingKind::Variable);
            }
            for inp in &input.input {
                self.declare(inp.identifier.name, inp.identifier.span, BindingKind::Variable);
            }
        }
        for cp in &input.const_parameters {
            self.visit_type(&cp.type_);
        }
        for inp in &input.input {
            self.visit_type(&inp.type_);
        }
        for out in &input.output {
            self.visit_type(&out.type_);
        }
        self.visit_type(&input.output_type);
        self.visit_block(&input.block);
        self.exit_scope();
    }

    // `visit_constructor` and `visit_interface` use the default `UnitVisitor` impls:
    // the constructor's body block already opens its own scope via `visit_block`, and the
    // interface walk only visits types.

    fn visit_stub(&mut self, _input: &Stub) {
        // Imported programs/libraries are not the user's code; skip.
    }

    fn visit_function_stub(&mut self, _input: &leo_ast::FunctionStub) {}

    fn visit_composite_stub(&mut self, _input: &Composite) {}
}

/// Collect the composite `Location`s reachable from `ty` through type wrappers
/// (Array, Optional, Tuple, Vector, Mapping, Future). Mirrors
/// `type_checking::add_composite_dependencies`.
fn collect_type_composite_refs(ty: &Type, refs: &mut IndexSet<Location>) {
    match ty {
        Type::Composite(c) => {
            if let Some(loc) = c.path.try_global_location() {
                refs.insert(loc.clone());
            }
        }
        Type::Array(a) => collect_type_composite_refs(a.element_type(), refs),
        Type::Optional(OptionalType { inner }) => collect_type_composite_refs(inner, refs),
        Type::Tuple(t) => {
            for elem in t.elements() {
                collect_type_composite_refs(elem, refs);
            }
        }
        Type::Vector(v) => collect_type_composite_refs(v.element_type(), refs),
        Type::Mapping(m) => {
            collect_type_composite_refs(&m.key, refs);
            collect_type_composite_refs(&m.value, refs);
        }
        Type::Future(f) => {
            for inp in &f.inputs {
                collect_type_composite_refs(inp, refs);
            }
        }
        _ => {}
    }
}
