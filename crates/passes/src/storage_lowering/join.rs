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

use leo_ast::{
    AstReconstructor,
    AstVisitor,
    DefinitionPlace,
    DynamicOpExpression,
    Expression,
    Identifier,
    Intrinsic,
    IntrinsicExpression,
    NodeID,
    Path,
    Statement,
    Type,
    UnitVisitor,
    Variant,
};
use leo_span::Symbol;

use indexmap::{IndexMap, IndexSet};

pub(super) fn contains_local_vector_join_op(expression: &Expression, program: Symbol) -> bool {
    let mut detector = LocalVectorJoinOpDetector { program, found: false };
    detector.visit_expression(expression, &());
    detector.found
}

struct LocalVectorJoinOpDetector {
    program: Symbol,
    found: bool,
}

impl AstVisitor for LocalVectorJoinOpDetector {
    type AdditionalInput = ();
    type Output = ();

    fn visit_dynamic_op(&mut self, _: &DynamicOpExpression, _: &Self::AdditionalInput) -> Self::Output {
        // Dynamic resource identity is outside this local-static vertical slice.
    }

    fn visit_intrinsic(&mut self, input: &IntrinsicExpression, _: &Self::AdditionalInput) -> Self::Output {
        self.found |= matches!(
            Intrinsic::from_symbol(input.name, &input.type_parameters),
            Some(Intrinsic::VectorPop | Intrinsic::VectorGet)
        ) && input.arguments.first().is_some_and(|receiver| {
            matches!(receiver, Expression::Path(path)
                if path.try_global_location().is_some_and(|location| location.program == self.program)
                    && path.user_program().is_none())
        });
        if !self.found {
            for argument in &input.arguments {
                self.visit_expression(argument, &());
            }
        }
    }
}

impl UnitVisitor for LocalVectorJoinOpDetector {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LoweringContext {
    Finalize,
    FinalFn,
    View,
    Constructor,
    #[default]
    Other,
}

impl From<Variant> for LoweringContext {
    fn from(variant: Variant) -> Self {
        match variant {
            Variant::Finalize => Self::Finalize,
            Variant::FinalFn => Self::FinalFn,
            Variant::View => Self::View,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ValueId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BindingId {
    scope: NodeID,
    symbol: Symbol,
}

#[derive(Clone, Copy, Debug)]
struct BindingState {
    id: BindingId,
    value: ValueId,
}

#[derive(Clone, Debug)]
struct LexicalScope {
    id: NodeID,
    bindings: IndexMap<Symbol, BindingState>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LexicalEnvironment {
    scopes: Vec<LexicalScope>,
    next_value: u32,
}

impl LexicalEnvironment {
    pub(super) fn enter_scope(&mut self, id: NodeID) {
        self.scopes.push(LexicalScope { id, bindings: IndexMap::new() });
    }

    pub(super) fn exit_scope(&mut self, id: NodeID) {
        let scope = self.scopes.pop().expect("a StorageLowering lexical scope must be active");
        assert_eq!(scope.id, id, "StorageLowering lexical scopes must be exited in stack order");
    }

    pub(super) fn observe_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Definition(input) => match &input.place {
                DefinitionPlace::Single(identifier) => {
                    self.define_current(identifier.name);
                }
                DefinitionPlace::Multiple(identifiers) => {
                    for identifier in identifiers {
                        self.define_current(identifier.name);
                    }
                }
            },
            Statement::Assign(input) => {
                if let Expression::Path(path) = &input.place
                    && let Some(symbol) = path.try_local_symbol()
                {
                    self.assign(symbol);
                }
            }
            _ => {}
        }
    }

    fn current_scope_id(&self) -> Option<NodeID> {
        self.scopes.last().map(|scope| scope.id)
    }

    fn define_current(&mut self, symbol: Symbol) -> Option<BindingState> {
        let scope = self.scopes.last_mut()?;
        if let Some(state) = scope.bindings.get(&symbol) {
            // Existing CPS paths can replay one source declaration into generated
            // sibling branches. It remains one logical source binding here; the
            // generated block scopes distinguish its materialized definitions.
            return Some(*state);
        }
        let id = BindingId { scope: scope.id, symbol };
        let value = ValueId(self.next_value);
        self.next_value = self.next_value.checked_add(1).expect("StorageLowering ValueId overflow");
        let state = BindingState { id, value };
        scope.bindings.insert(symbol, state);
        Some(state)
    }

    fn assign(&mut self, symbol: Symbol) -> Option<BindingState> {
        let index = self.scopes.iter().rposition(|scope| scope.bindings.contains_key(&symbol))?;
        let state = self.scopes[index].bindings.get_mut(&symbol).expect("binding index was just resolved");
        state.value = ValueId(self.next_value);
        self.next_value = self.next_value.checked_add(1).expect("StorageLowering ValueId overflow");
        Some(*state)
    }

    fn resolve(&self, symbol: Symbol) -> Option<BindingState> {
        self.scopes.iter().rev().find_map(|scope| scope.bindings.get(&symbol).copied())
    }
}

#[derive(Clone, Debug)]
struct JoinParameter {
    binding: BindingId,
    value: ValueId,
    type_: Type,
}

#[derive(Clone, Debug)]
enum PlannedStatementKind {
    Ordinary,
    DirectAssignment { binding: BindingId },
}

#[derive(Clone, Debug)]
struct PlannedStatement {
    statement: Statement,
    uses: IndexSet<ValueId>,
    defines: IndexSet<ValueId>,
    kind: PlannedStatementKind,
}

#[derive(Clone, Debug)]
pub(super) struct JoinPlan {
    parameter: JoinParameter,
    source_scope: NodeID,
    dependent_prefix: Vec<PlannedStatement>,
    shared_suffix: Vec<Statement>,
}

impl JoinPlan {
    pub(super) fn build(
        environment: &mut LexicalEnvironment,
        parameter: Identifier,
        parameter_type: Type,
        tail: &[Statement],
    ) -> Option<Self> {
        let source_scope = environment.current_scope_id()?;
        let parameter_state = environment.define_current(parameter.name)?;
        let parameter =
            JoinParameter { binding: parameter_state.id, value: parameter_state.value, type_: parameter_type };

        let mut analysis = environment.clone();
        let mut statements = Vec::with_capacity(tail.len());
        for statement in tail {
            statements.push(plan_statement(statement.clone(), &mut analysis, source_scope)?);
        }
        environment.next_value = analysis.next_value;

        let cut = dependent_cut(&statements, &[parameter.value]);
        let (dependent_prefix, shared_suffix) = match cut {
            Some(cut) => {
                (statements[..=cut].to_vec(), statements[cut + 1..].iter().map(|step| step.statement.clone()).collect())
            }
            None => (Vec::new(), statements.iter().map(|step| step.statement.clone()).collect()),
        };

        Some(Self { parameter, source_scope, dependent_prefix, shared_suffix })
    }

    pub(super) fn validate_arm(&self, parameter_symbol: Symbol, actual: &Type) {
        assert_eq!(
            self.parameter.binding.symbol, parameter_symbol,
            "a StorageLowering join arm must bind the planned lexical parameter"
        );
        assert_eq!(
            &self.parameter.type_, actual,
            "type checking must make every StorageLowering join argument match its parameter"
        );
    }

    pub(super) fn shared_suffix(&self) -> &[Statement] {
        &self.shared_suffix
    }

    pub(super) fn has_branching_prefix(&self, predicate: impl Fn(&Statement) -> bool) -> bool {
        self.dependent_prefix.iter().any(|step| predicate(&step.statement))
    }

    pub(super) fn versioned_prefix(&self, state: &mut CompilerState) -> Vec<Statement> {
        let mut rewriter = JoinPrefixVersioner::new(state, self.source_scope);
        let mut statements = Vec::with_capacity(self.dependent_prefix.len());

        for step in &self.dependent_prefix {
            match step.kind {
                PlannedStatementKind::Ordinary => {
                    statements.push(rewriter.reconstruct_statement(step.statement.clone()).0);
                }
                PlannedStatementKind::DirectAssignment { binding } => {
                    let Statement::Assign(input) = step.statement.clone() else {
                        unreachable!("a direct-assignment plan step must contain an assignment")
                    };
                    let Expression::Path(place) = input.place else {
                        unreachable!("a direct-assignment plan step must target a path")
                    };
                    let value = rewriter.reconstruct_expression(input.value, &()).0;
                    let type_ = rewriter
                        .state
                        .type_table
                        .get(&place.id())
                        .expect("type checking must assign a type to a local assignment target");
                    let symbol = rewriter.state.assigner.unique_symbol(place.identifier().name, "$join");
                    let identifier = Identifier {
                        name: symbol,
                        span: place.identifier().span,
                        id: rewriter.state.node_builder.next_id(),
                    };
                    rewriter.state.type_table.insert(identifier.id, type_.clone());
                    statements.push(rewriter.state.assigner.simple_definition(
                        identifier,
                        value,
                        rewriter.state.node_builder.next_id(),
                    ));
                    rewriter.bind(binding, identifier, type_.clone());
                }
            }
        }

        statements
    }
}

fn plan_statement(
    statement: Statement,
    environment: &mut LexicalEnvironment,
    source_scope: NodeID,
) -> Option<PlannedStatement> {
    let mut uses = match &statement {
        Statement::Assign(input) if matches!(input.place, Expression::Path(_)) => {
            collect_expression_uses(&input.value, environment)
        }
        Statement::Definition(input) => collect_expression_uses(&input.value, environment),
        Statement::Block(_) | Statement::Conditional(_) | Statement::Const(_) | Statement::Iteration(_) => return None,
        _ => collect_statement_uses(&statement, environment),
    };

    let mut defines = IndexSet::new();
    let kind = match &statement {
        Statement::Definition(input) => {
            let DefinitionPlace::Single(identifier) = input.place else {
                return None;
            };
            defines.insert(environment.define_current(identifier.name)?.value);
            PlannedStatementKind::Ordinary
        }
        Statement::Assign(input) => match &input.place {
            Expression::Path(path) => {
                let Some(symbol) = path.try_local_symbol() else {
                    return Some(PlannedStatement { statement, uses, defines, kind: PlannedStatementKind::Ordinary });
                };
                let binding_state = environment.resolve(symbol)?;
                let binding = binding_state.id;
                if binding.scope != source_scope {
                    return None;
                }
                // A direct overwrite does not read the previous value, but it still
                // requires the binding to exist in the materialized scope. Treat that
                // ownership dependency as a use so a join parameter or prefix-local
                // definition cannot be assigned from a shared suffix.
                uses.insert(binding_state.value);
                defines.insert(environment.assign(symbol)?.value);
                PlannedStatementKind::DirectAssignment { binding }
            }
            _ => return None,
        },
        _ => PlannedStatementKind::Ordinary,
    };

    Some(PlannedStatement { statement, uses, defines, kind })
}

fn dependent_cut(statements: &[PlannedStatement], parameters: &[ValueId]) -> Option<usize> {
    let parameters: IndexSet<ValueId> = parameters.iter().copied().collect();
    let mut cut = statements
        .iter()
        .enumerate()
        .filter(|(_, statement)| !statement.uses.is_disjoint(&parameters))
        .map(|(index, _)| index)
        .next_back();

    while let Some(index) = cut {
        let prefix_definitions: IndexSet<ValueId> =
            statements[..=index].iter().flat_map(|statement| statement.defines.iter().copied()).collect();
        match statements
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, statement)| !statement.uses.is_disjoint(&prefix_definitions))
        {
            Some((next, _)) => cut = Some(next),
            None => break,
        }
    }

    cut
}

fn collect_expression_uses(expression: &Expression, environment: &LexicalEnvironment) -> IndexSet<ValueId> {
    let mut collector = ValueUseCollector { environment, uses: IndexSet::new() };
    collector.visit_expression(expression, &());
    collector.uses
}

fn collect_statement_uses(statement: &Statement, environment: &LexicalEnvironment) -> IndexSet<ValueId> {
    let mut collector = ValueUseCollector { environment, uses: IndexSet::new() };
    collector.visit_statement(statement);
    collector.uses
}

struct ValueUseCollector<'a> {
    environment: &'a LexicalEnvironment,
    uses: IndexSet<ValueId>,
}

impl AstVisitor for ValueUseCollector<'_> {
    type AdditionalInput = ();
    type Output = ();

    fn visit_path(&mut self, input: &Path, _: &Self::AdditionalInput) -> Self::Output {
        if let Some(symbol) = input.try_local_symbol()
            && let Some(binding) = self.environment.resolve(symbol)
        {
            self.uses.insert(binding.value);
        }
    }
}

impl UnitVisitor for ValueUseCollector<'_> {}

#[derive(Clone)]
struct Replacement {
    identifier: Identifier,
    type_: Type,
}

struct JoinPrefixVersioner<'a> {
    state: &'a mut CompilerState,
    source_scope: NodeID,
    replacements: IndexMap<BindingId, Replacement>,
}

impl<'a> JoinPrefixVersioner<'a> {
    fn new(state: &'a mut CompilerState, source_scope: NodeID) -> Self {
        Self { state, source_scope, replacements: IndexMap::new() }
    }

    fn bind(&mut self, binding: BindingId, identifier: Identifier, type_: Type) {
        self.replacements.insert(binding, Replacement { identifier, type_ });
    }
}

impl AstReconstructor for JoinPrefixVersioner<'_> {
    type AdditionalInput = ();
    type AdditionalOutput = ();

    fn reconstruct_path(&mut self, input: Path, _: &Self::AdditionalInput) -> (Expression, Self::AdditionalOutput) {
        let Some(symbol) = input.try_local_symbol() else {
            return (input.into(), ());
        };
        let binding = BindingId { scope: self.source_scope, symbol };
        let Some(replacement) = self.replacements.get(&binding) else {
            return (input.into(), ());
        };

        let identifier =
            Identifier { name: replacement.identifier.name, span: input.span, id: self.state.node_builder.next_id() };
        let path = Path::from(identifier).to_local();
        self.state.type_table.insert(path.id(), replacement.type_.clone());
        (path.into(), ())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct JoinMaterializationCost {
    pub(super) top_level_statements: usize,
    pub(super) statement_nodes: usize,
}

impl JoinMaterializationCost {
    pub(super) fn from_materialized(selected_region: &[Statement], shared_suffix: &[Statement]) -> Option<Self> {
        let selected_nodes = statement_slice_nodes(selected_region)?;
        let shared_nodes = statement_slice_nodes(shared_suffix)?;
        checked_materialization_cost(selected_region.len(), shared_suffix.len(), selected_nodes, shared_nodes)
    }
}

fn checked_materialization_cost(
    selected_top_level: usize,
    shared_top_level: usize,
    selected_nodes: usize,
    shared_nodes: usize,
) -> Option<JoinMaterializationCost> {
    Some(JoinMaterializationCost {
        top_level_statements: selected_top_level.checked_add(shared_top_level)?,
        statement_nodes: selected_nodes.checked_add(shared_nodes)?,
    })
}

fn statement_slice_nodes(statements: &[Statement]) -> Option<usize> {
    statements.iter().try_fold(0usize, |count, statement| count.checked_add(statement_nodes(statement)?))
}

fn statement_nodes(statement: &Statement) -> Option<usize> {
    let children = match statement {
        Statement::Block(block) => statement_slice_nodes(&block.statements)?,
        Statement::Conditional(conditional) => {
            let then_nodes = statement_slice_nodes(&conditional.then.statements)?;
            let otherwise_nodes = match conditional.otherwise.as_deref() {
                Some(otherwise) => statement_nodes(otherwise)?,
                None => 0,
            };
            then_nodes.checked_add(otherwise_nodes)?
        }
        _ => 0,
    };
    1usize.checked_add(children)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_statement() -> Statement {
        leo_ast::ExpressionStatement {
            expression: leo_ast::UnitExpression { span: Default::default(), id: 0 }.into(),
            span: Default::default(),
            id: 0,
        }
        .into()
    }

    #[test]
    fn lexical_identity_includes_the_defining_scope() {
        leo_span::create_session_if_not_set_then(|_| {
            let symbol = Symbol::intern("value");
            assert_ne!(BindingId { scope: 1, symbol }, BindingId { scope: 2, symbol });

            let mut environment = LexicalEnvironment::default();
            environment.enter_scope(1);
            let first = environment.define_current(symbol).unwrap();
            let replay = environment.define_current(symbol).unwrap();
            assert_eq!(first.id, replay.id);
            assert_eq!(first.value, replay.value);
        });
    }

    #[test]
    fn dependent_cut_closes_over_interleaved_definitions() {
        let parameter = ValueId(1);
        let derived = ValueId(2);
        let statements = vec![
            PlannedStatement {
                statement: unit_statement(),
                uses: IndexSet::new(),
                defines: IndexSet::from([derived]),
                kind: PlannedStatementKind::Ordinary,
            },
            PlannedStatement {
                statement: unit_statement(),
                uses: IndexSet::from([parameter]),
                defines: IndexSet::new(),
                kind: PlannedStatementKind::Ordinary,
            },
            PlannedStatement {
                statement: unit_statement(),
                uses: IndexSet::from([derived]),
                defines: IndexSet::new(),
                kind: PlannedStatementKind::Ordinary,
            },
        ];
        assert_eq!(dependent_cut(&statements, &[parameter]), Some(2));
    }

    #[test]
    fn materialization_cost_uses_checked_arithmetic() {
        assert_eq!(
            checked_materialization_cost(1, 2, 4, 5),
            Some(JoinMaterializationCost { top_level_statements: 3, statement_nodes: 9 })
        );
        assert_eq!(checked_materialization_cost(usize::MAX, 1, 0, 0), None);
        assert_eq!(checked_materialization_cost(0, 0, usize::MAX, 1), None);
    }

    #[test]
    fn materialization_cost_counts_the_real_statement_tree() {
        let selected_region = vec![
            leo_ast::ConditionalStatement {
                condition: leo_ast::UnitExpression { span: Default::default(), id: 0 }.into(),
                then: leo_ast::Block {
                    statements: vec![unit_statement(), unit_statement()],
                    span: Default::default(),
                    id: 0,
                },
                otherwise: Some(Box::new(
                    leo_ast::Block { statements: vec![unit_statement()], span: Default::default(), id: 0 }.into(),
                )),
                span: Default::default(),
                id: 0,
            }
            .into(),
        ];
        assert_eq!(
            JoinMaterializationCost::from_materialized(&selected_region, &[unit_statement()]),
            Some(JoinMaterializationCost { top_level_statements: 2, statement_nodes: 6 })
        );
    }
}
