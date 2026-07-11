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

use super::StorageLoweringVisitor;
use crate::{SymbolAccessCollector, expression_can_be_discarded};

use leo_ast::*;
use leo_span::{Span, Symbol, sym};

use std::rc::Rc;

type ExpressionContinuation = Rc<dyn for<'a> Fn(Expression, &mut StorageLoweringVisitor<'a>) -> Vec<Statement>>;
type ExpressionListContinuation =
    Rc<dyn for<'a> Fn(Vec<Expression>, &mut StorageLoweringVisitor<'a>) -> Vec<Statement>>;
type OptionalExpressionContinuation =
    Rc<dyn for<'a> Fn(Option<Box<Expression>>, &mut StorageLoweringVisitor<'a>) -> Vec<Statement>>;

impl StorageLoweringVisitor<'_> {
    fn copy_expression_type(&mut self, from: leo_ast::NodeID, to: leo_ast::NodeID) {
        if from != to
            && let Some(type_) = self.state.type_table.get(&from)
        {
            self.state.type_table.insert(to, type_);
        }
    }

    /// Lower `expression` in a context that consumes its value.
    ///
    /// `AstReconstructor::AdditionalOutput` is an unconditional prelude. It is only sound for
    /// subexpressions that are definitely evaluated in the current dynamic path. A source ternary's
    /// condition is definitely evaluated, but its arms are not: statements produced by lowering the
    /// true arm must execute exactly when the true arm is selected, and likewise for the false arm.
    ///
    /// Statement owners therefore use this continuation-passing emitter instead of the eager
    /// expression reconstructor. The ternary rule is the invariant-preserving one:
    ///
    /// ```text
    /// emit(c ? t : f, K) = emit(c, |c'| if c' { emit(t, K) } else { emit(f, K) })
    /// ```
    ///
    /// This preserves strict source evaluation order for expression prefixes. Strict-prefix
    /// contexts materialize non-discardable values before lowering the next sibling, while
    /// ternary branches keep branch-produced effects inside the branch that selected them. When
    /// a branch-local definition is used by following statements, statement lowering replays only
    /// the tail prefix that still depends on branch-local values and rejoins after that prefix.
    fn emit_expression_with_continuation(
        &mut self,
        expression: Expression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        match expression {
            Expression::Array(input) => {
                let mut input = input;
                let elements = std::mem::take(&mut input.elements);
                self.emit_expression_list_with_continuation(
                    elements,
                    Vec::new(),
                    Rc::new(move |elements, this| {
                        let mut input = input.clone();
                        input.elements = elements;
                        continuation.clone()(input.into(), this)
                    }),
                )
            }
            Expression::ArrayAccess(input) => {
                let ArrayAccess { array, index, span, id } = *input;
                let index_may_emit = self.expression_may_emit_after_reconstruction(&index);
                self.emit_expression_with_continuation(
                    array,
                    Rc::new(move |array, this| {
                        let mut statements = Vec::new();
                        let array = if index_may_emit {
                            this.materialize_strict_prefix_expression(array, &mut statements)
                        } else {
                            array
                        };
                        let index = index.clone();
                        let continuation = continuation.clone();
                        statements.extend(this.emit_expression_with_continuation(
                            index,
                            Rc::new(move |index, this| {
                                continuation.clone()(ArrayAccess { array: array.clone(), index, span, id }.into(), this)
                            }),
                        ));
                        statements
                    }),
                )
            }
            Expression::Binary(input) => {
                let BinaryExpression { left, right, op, span, id } = *input;
                let right_may_emit = self.expression_may_emit_after_reconstruction(&right);
                self.emit_expression_with_continuation(
                    left,
                    Rc::new(move |left, this| {
                        let mut statements = Vec::new();
                        let left = if right_may_emit {
                            this.materialize_strict_prefix_expression(left, &mut statements)
                        } else {
                            left
                        };
                        let right = right.clone();
                        let continuation = continuation.clone();
                        statements.extend(this.emit_expression_with_continuation(
                            right,
                            Rc::new(move |right, this| {
                                continuation.clone()(
                                    BinaryExpression { left: left.clone(), right, op, span, id }.into(),
                                    this,
                                )
                            }),
                        ));
                        statements
                    }),
                )
            }
            Expression::Call(input) => {
                let mut input = *input;
                let const_arguments = std::mem::take(&mut input.const_arguments);
                let arguments = std::mem::take(&mut input.arguments);
                self.emit_expression_list_with_continuation(
                    const_arguments,
                    Vec::new(),
                    Rc::new(move |const_arguments, this| {
                        let input = input.clone();
                        let arguments = arguments.clone();
                        let continuation = continuation.clone();
                        this.emit_expression_list_with_continuation(
                            arguments,
                            Vec::new(),
                            Rc::new(move |arguments, this| {
                                let mut input = input.clone();
                                input.const_arguments = const_arguments.clone();
                                input.arguments = arguments;
                                this.emit_reconstructed_call(input, continuation.clone())
                            }),
                        )
                    }),
                )
            }
            Expression::Cast(input) => {
                let CastExpression { expression, type_, span, id } = *input;
                self.emit_expression_with_continuation(
                    expression,
                    Rc::new(move |expression, this| {
                        continuation.clone()(CastExpression { expression, type_: type_.clone(), span, id }.into(), this)
                    }),
                )
            }
            Expression::Composite(input) => self.emit_composite_with_continuation(input, continuation),
            Expression::DynamicOp(input) => self.emit_dynamic_op_with_continuation(*input, continuation),
            Expression::Intrinsic(input) => self.emit_intrinsic_with_continuation(*input, continuation),
            Expression::MemberAccess(input) => {
                let MemberAccess { inner, name, span, id } = *input;
                self.emit_expression_with_continuation(
                    inner,
                    Rc::new(move |inner, this| {
                        continuation.clone()(MemberAccess { inner, name, span, id }.into(), this)
                    }),
                )
            }
            Expression::Repeat(input) => {
                let RepeatExpression { expr, count, span, id } = *input;
                let count_may_emit = self.expression_may_emit_after_reconstruction(&count);
                self.emit_expression_with_continuation(
                    expr,
                    Rc::new(move |expr, this| {
                        let mut statements = Vec::new();
                        let expr = if count_may_emit {
                            this.materialize_strict_prefix_expression(expr, &mut statements)
                        } else {
                            expr
                        };
                        let count = count.clone();
                        let continuation = continuation.clone();
                        statements.extend(this.emit_expression_with_continuation(
                            count,
                            Rc::new(move |count, this| {
                                continuation.clone()(
                                    RepeatExpression { expr: expr.clone(), count, span, id }.into(),
                                    this,
                                )
                            }),
                        ));
                        statements
                    }),
                )
            }
            Expression::Ternary(input) => self.emit_ternary_with_continuation(*input, continuation),
            Expression::Tuple(input) => {
                let mut input = input;
                let elements = std::mem::take(&mut input.elements);
                self.emit_expression_list_with_continuation(
                    elements,
                    Vec::new(),
                    Rc::new(move |elements, this| {
                        let mut input = input.clone();
                        input.elements = elements;
                        continuation.clone()(input.into(), this)
                    }),
                )
            }
            Expression::TupleAccess(input) => {
                let TupleAccess { tuple, index, span, id } = *input;
                self.emit_expression_with_continuation(
                    tuple,
                    Rc::new(move |tuple, this| {
                        continuation.clone()(TupleAccess { tuple, index: index.clone(), span, id }.into(), this)
                    }),
                )
            }
            Expression::Unary(input) => {
                let UnaryExpression { receiver, op, span, id } = *input;
                self.emit_expression_with_continuation(
                    receiver,
                    Rc::new(move |receiver, this| {
                        continuation.clone()(UnaryExpression { op, receiver, span, id }.into(), this)
                    }),
                )
            }
            expression => self.emit_reconstructed_expression(expression, continuation),
        }
    }

    fn emit_reconstructed_expression(
        &mut self,
        expression: Expression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let source_id = expression.id();
        let (expression, mut statements) = self.reconstruct_expression(expression, &());
        self.copy_expression_type(source_id, expression.id());
        if Self::expression_requires_branch_cps(&expression) {
            statements.extend(self.emit_expression_with_continuation(expression, continuation));
        } else {
            statements.extend(continuation(expression, self));
        }
        statements
    }

    fn emit_reconstructed_call(
        &mut self,
        input: CallExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let source_id = input.id;
        let (expression, mut statements) = self.reconstruct_call(input, &());
        self.copy_expression_type(source_id, expression.id());
        if Self::expression_requires_branch_cps(&expression) {
            statements.extend(self.emit_expression_with_continuation(expression, continuation));
        } else {
            statements.extend(continuation(expression, self));
        }
        statements
    }

    fn emit_reconstructed_dynamic_op(
        &mut self,
        input: DynamicOpExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let source_id = input.id;
        let (expression, mut statements) = self.reconstruct_dynamic_op(input, &());
        self.copy_expression_type(source_id, expression.id());
        if !matches!(expression, Expression::DynamicOp(_)) && Self::expression_requires_branch_cps(&expression) {
            statements.extend(self.emit_expression_with_continuation(expression, continuation));
        } else {
            statements.extend(continuation(expression, self));
        }
        statements
    }

    fn emit_reconstructed_intrinsic(
        &mut self,
        input: IntrinsicExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let source_id = input.id;
        let (expression, mut statements) = self.reconstruct_intrinsic(input, &());
        self.copy_expression_type(source_id, expression.id());
        if Self::expression_requires_branch_cps(&expression) {
            statements.extend(self.emit_expression_with_continuation(expression, continuation));
        } else {
            statements.extend(continuation(expression, self));
        }
        statements
    }

    fn materialize_strict_prefix_expression(
        &mut self,
        expression: Expression,
        statements: &mut Vec<Statement>,
    ) -> Expression {
        if expression_can_be_discarded(&expression, self.state) {
            return expression;
        }

        let type_ = self
            .expression_type_for_materialization(&expression)
            .expect("storage lowering should know the type before strict-prefix materialization");

        let temp_sym = self.state.assigner.unique_symbol("$eval", "$");
        let temp_ident = Identifier { name: temp_sym, span: Default::default(), id: self.state.node_builder.next_id() };
        self.state.type_table.insert(temp_ident.id, type_);
        statements.push(self.state.assigner.simple_definition(
            temp_ident,
            expression,
            self.state.node_builder.next_id(),
        ));
        Path::from(temp_ident).to_local().into()
    }

    fn expression_may_emit_cps_statements(expression: &Expression) -> bool {
        match expression {
            Expression::Array(input) => input.elements.iter().any(Self::expression_may_emit_cps_statements),
            Expression::ArrayAccess(input) => {
                Self::expression_may_emit_cps_statements(&input.array)
                    || Self::expression_may_emit_cps_statements(&input.index)
            }
            Expression::Binary(input) => {
                Self::expression_may_emit_cps_statements(&input.left)
                    || Self::expression_may_emit_cps_statements(&input.right)
            }
            Expression::Call(input) => {
                input.const_arguments.iter().any(Self::expression_may_emit_cps_statements)
                    || input.arguments.iter().any(Self::expression_may_emit_cps_statements)
            }
            Expression::Cast(input) => Self::expression_may_emit_cps_statements(&input.expression),
            Expression::Composite(input) => {
                input.const_arguments.iter().any(Self::expression_may_emit_cps_statements)
                    || input
                        .members
                        .iter()
                        .filter_map(|member| member.expression.as_ref())
                        .any(Self::expression_may_emit_cps_statements)
                    || input.base.as_deref().is_some_and(Self::expression_may_emit_cps_statements)
            }
            Expression::DynamicOp(_) => true,
            Expression::Intrinsic(input) => {
                matches!(
                    Intrinsic::from_symbol(input.name, &input.type_parameters),
                    Some(
                        Intrinsic::MappingGet
                            | Intrinsic::MappingGetOrUse
                            | Intrinsic::MappingContains
                            | Intrinsic::MappingSet
                            | Intrinsic::MappingRemove
                            | Intrinsic::DynamicGet
                            | Intrinsic::DynamicGetOrUse
                            | Intrinsic::DynamicContains
                            | Intrinsic::VectorPush
                            | Intrinsic::VectorPop
                            | Intrinsic::VectorGet
                            | Intrinsic::VectorSet
                            | Intrinsic::VectorLen
                            | Intrinsic::VectorClear
                            | Intrinsic::VectorSwapRemove
                    )
                ) || input.arguments.iter().any(Self::expression_may_emit_cps_statements)
            }
            Expression::MemberAccess(input) => Self::expression_may_emit_cps_statements(&input.inner),
            Expression::Repeat(input) => {
                Self::expression_may_emit_cps_statements(&input.expr)
                    || Self::expression_may_emit_cps_statements(&input.count)
            }
            Expression::Ternary(input) => {
                Self::expression_may_emit_cps_statements(&input.condition)
                    || Self::expression_may_emit_cps_statements(&input.if_true)
                    || Self::expression_may_emit_cps_statements(&input.if_false)
            }
            Expression::Tuple(input) => input.elements.iter().any(Self::expression_may_emit_cps_statements),
            Expression::TupleAccess(input) => Self::expression_may_emit_cps_statements(&input.tuple),
            Expression::Unary(input) => Self::expression_may_emit_cps_statements(&input.receiver),
            _ => false,
        }
    }

    fn emit_expression_list_with_continuation(
        &mut self,
        remaining: Vec<Expression>,
        rebuilt: Vec<Expression>,
        continuation: ExpressionListContinuation,
    ) -> Vec<Statement> {
        let mut remaining_iter = remaining.into_iter();
        let Some(next) = remaining_iter.next() else {
            return continuation(rebuilt, self);
        };
        let rest = remaining_iter.collect::<Vec<_>>();

        self.emit_expression_with_continuation(
            next,
            Rc::new(move |next, this| {
                let mut statements = Vec::new();
                let rest_may_emit =
                    rest.iter().any(|expression| this.expression_may_emit_after_reconstruction(expression));
                let next =
                    if rest_may_emit { this.materialize_strict_prefix_expression(next, &mut statements) } else { next };
                let mut rebuilt = rebuilt.clone();
                rebuilt.push(next);
                statements.extend(this.emit_expression_list_with_continuation(
                    rest.clone(),
                    rebuilt,
                    continuation.clone(),
                ));
                statements
            }),
        )
    }

    fn emit_optional_boxed_expression_with_continuation(
        &mut self,
        expression: Option<Box<Expression>>,
        continuation: OptionalExpressionContinuation,
    ) -> Vec<Statement> {
        match expression {
            Some(expression) => self.emit_expression_with_continuation(
                *expression,
                Rc::new(move |expression, this| continuation.clone()(Some(Box::new(expression)), this)),
            ),
            None => continuation(None, self),
        }
    }

    fn emit_ternary_with_continuation(
        &mut self,
        input: TernaryExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let TernaryExpression { condition, if_true, if_false, span, id } = input;
        let optional_type = match self.state.type_table.get(&id) {
            Some(Type::Optional(optional)) => Some(Type::Optional(optional)),
            _ => None,
        };
        let true_arm_may_emit = self.expression_may_emit_after_reconstruction(&if_true);
        let false_arm_may_emit = self.expression_may_emit_after_reconstruction(&if_false);
        if !true_arm_may_emit && !false_arm_may_emit {
            let optional_type_for_reconstructed = optional_type.clone();
            let if_true_for_reconstruction = if_true.clone();
            let if_false_for_reconstruction = if_false.clone();
            return self.emit_expression_with_continuation(
                condition,
                Rc::new(move |condition, this| {
                    let (if_true, true_prefix) = this.reconstruct_expression(if_true_for_reconstruction.clone(), &());
                    let (if_false, false_prefix) =
                        this.reconstruct_expression(if_false_for_reconstruction.clone(), &());

                    if true_prefix.is_empty()
                        && false_prefix.is_empty()
                        && !Self::expression_requires_branch_cps(&if_true)
                        && !Self::expression_requires_branch_cps(&if_false)
                    {
                        continuation.clone()(TernaryExpression { condition, if_true, if_false, span, id }.into(), this)
                    } else {
                        this.emit_ternary_branches_with_continuation(
                            TernaryExpression { condition, if_true, if_false, span, id },
                            true_prefix,
                            false_prefix,
                            optional_type_for_reconstructed.clone(),
                            continuation.clone(),
                        )
                    }
                }),
            );
        }

        self.emit_expression_with_continuation(
            condition,
            Rc::new(move |condition, this| {
                this.emit_ternary_branches_with_continuation(
                    TernaryExpression { condition, if_true: if_true.clone(), if_false: if_false.clone(), span, id },
                    Vec::new(),
                    Vec::new(),
                    optional_type.clone(),
                    continuation.clone(),
                )
            }),
        )
    }

    fn emit_ternary_branches_with_continuation(
        &mut self,
        input: TernaryExpression,
        true_prefix: Vec<Statement>,
        false_prefix: Vec<Statement>,
        optional_type: Option<Type>,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let TernaryExpression { condition, if_true, if_false, span, .. } = input;
        let optional_type_for_true = optional_type.clone();
        let optional_type_for_false = optional_type.clone();
        let true_continuation = continuation.clone();
        let false_continuation = continuation.clone();

        let mut then_statements = true_prefix;
        then_statements.extend(self.emit_expression_with_continuation(
            if_true,
            Rc::new(move |if_true, this| {
                let mut statements = Vec::new();
                let if_true = if let Some(optional_type) = optional_type_for_true.clone() {
                    let (if_true, branch_statements) = this.bind_optional_ternary_branch(if_true, optional_type, span);
                    statements.extend(branch_statements);
                    if_true
                } else {
                    if_true
                };
                statements.extend(true_continuation.clone()(if_true, this));
                statements
            }),
        ));

        let mut else_statements = false_prefix;
        else_statements.extend(self.emit_expression_with_continuation(
            if_false,
            Rc::new(move |if_false, this| {
                let mut statements = Vec::new();
                let if_false = if let Some(optional_type) = optional_type_for_false.clone() {
                    let (if_false, branch_statements) =
                        this.bind_optional_ternary_branch(if_false, optional_type, span);
                    statements.extend(branch_statements);
                    if_false
                } else {
                    if_false
                };
                statements.extend(false_continuation.clone()(if_false, this));
                statements
            }),
        ));

        vec![
            ConditionalStatement {
                condition,
                then: Block { statements: then_statements, span, id: self.state.node_builder.next_id() },
                otherwise: Some(Box::new(
                    Block { statements: else_statements, span, id: self.state.node_builder.next_id() }.into(),
                )),
                span,
                id: self.state.node_builder.next_id(),
            }
            .into(),
        ]
    }

    fn bind_optional_ternary_branch(
        &mut self,
        expression: Expression,
        optional_type: Type,
        span: Span,
    ) -> (Expression, Vec<Statement>) {
        let temp_sym = self.state.assigner.unique_symbol("$ternary_branch", "$");
        let temp_ident = Identifier { name: temp_sym, span: Default::default(), id: self.state.node_builder.next_id() };
        self.state.type_table.insert(temp_ident.id, optional_type.clone());

        let definition = DefinitionStatement {
            place: DefinitionPlace::Single(temp_ident),
            type_: Some(optional_type.clone()),
            value: expression,
            span,
            id: self.state.node_builder.next_id(),
        };

        let path: Expression = Path::from(temp_ident).to_local().into();
        self.state.type_table.insert(path.id(), optional_type);
        (path, vec![definition.into()])
    }

    fn emit_intrinsic_with_continuation(
        &mut self,
        mut input: IntrinsicExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        // Vector intrinsics use argument 0 as the storage vector path. The intrinsic-specific
        // lowering validates and consumes that path, so only the value/index suffix is evaluated by
        // the CPS emitter before the intrinsic is reconstructed.
        let first_value_argument = match Intrinsic::from_symbol(input.name, &input.type_parameters) {
            Some(Intrinsic::VectorPush)
            | Some(Intrinsic::VectorLen)
            | Some(Intrinsic::VectorPop)
            | Some(Intrinsic::VectorGet)
            | Some(Intrinsic::VectorSet)
            | Some(Intrinsic::VectorClear)
            | Some(Intrinsic::VectorSwapRemove) => 1,
            _ => 0,
        };

        let suffix = input.arguments.split_off(first_value_argument);
        self.emit_expression_list_with_continuation(
            suffix,
            Vec::new(),
            Rc::new(move |suffix, this| {
                let mut input = input.clone();
                input.arguments.extend(suffix);
                this.emit_reconstructed_intrinsic(input, continuation.clone())
            }),
        )
    }

    fn emit_dynamic_op_with_continuation(
        &mut self,
        mut input: DynamicOpExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let target_program = std::mem::take(&mut input.target_program);
        self.emit_expression_with_continuation(
            target_program,
            Rc::new(move |target_program, this| {
                let mut statements = Vec::new();
                let target_program = this.materialize_strict_prefix_expression(target_program, &mut statements);
                let mut input = input.clone();
                input.target_program = target_program;
                statements.extend(this.emit_dynamic_network_with_continuation(input, continuation.clone()));
                statements
            }),
        )
    }

    fn emit_dynamic_network_with_continuation(
        &mut self,
        mut input: DynamicOpExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        if let Some(network) = input.network.take() {
            self.emit_expression_with_continuation(
                network,
                Rc::new(move |network, this| {
                    let mut statements = Vec::new();
                    let network = this.materialize_strict_prefix_expression(network, &mut statements);
                    let mut input = input.clone();
                    input.network = Some(network);
                    statements.extend(this.emit_dynamic_arguments_with_continuation(input, continuation.clone()));
                    statements
                }),
            )
        } else {
            self.emit_dynamic_arguments_with_continuation(input, continuation)
        }
    }

    fn emit_dynamic_arguments_with_continuation(
        &mut self,
        mut input: DynamicOpExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let dynamic_arguments = match &mut input.kind {
            DynamicOpKind::Read { .. } => return self.emit_reconstructed_dynamic_op(input, continuation),
            DynamicOpKind::Op { arguments, .. } | DynamicOpKind::Call { arguments, .. } => std::mem::take(arguments),
        };

        self.emit_expression_list_with_continuation(
            dynamic_arguments,
            Vec::new(),
            Rc::new(move |arguments, this| {
                let mut input = input.clone();
                match &mut input.kind {
                    DynamicOpKind::Read { .. } => unreachable!("read dynamic ops do not have arguments"),
                    DynamicOpKind::Op { arguments: dynamic_arguments, .. }
                    | DynamicOpKind::Call { arguments: dynamic_arguments, .. } => {
                        *dynamic_arguments = arguments;
                    }
                }
                this.emit_reconstructed_dynamic_op(input, continuation.clone())
            }),
        )
    }

    fn emit_composite_with_continuation(
        &mut self,
        mut input: CompositeExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        let const_arguments = std::mem::take(&mut input.const_arguments);
        self.emit_expression_list_with_continuation(
            const_arguments,
            Vec::new(),
            Rc::new(move |const_arguments, this| {
                let mut input = input.clone();
                input.const_arguments = const_arguments;
                this.emit_composite_members_with_continuation(input, 0, continuation.clone())
            }),
        )
    }

    fn emit_composite_members_with_continuation(
        &mut self,
        mut input: CompositeExpression,
        index: usize,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        if index == input.members.len() {
            return self.emit_composite_base_with_continuation(input, continuation);
        }

        let Some(expression) = input.members[index].expression.take() else {
            return self.emit_composite_members_with_continuation(input, index + 1, continuation);
        };

        self.emit_expression_with_continuation(
            expression,
            Rc::new(move |expression, this| {
                let mut statements = Vec::new();
                let expression = this.materialize_strict_prefix_expression(expression, &mut statements);
                let mut input = input.clone();
                input.members[index].expression = Some(expression);
                statements.extend(this.emit_composite_members_with_continuation(
                    input,
                    index + 1,
                    continuation.clone(),
                ));
                statements
            }),
        )
    }

    fn emit_composite_base_with_continuation(
        &mut self,
        mut input: CompositeExpression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        self.emit_optional_boxed_expression_with_continuation(
            input.base.take(),
            Rc::new(move |base, this| {
                let mut input = input.clone();
                input.base = base;
                continuation.clone()(input.into(), this)
            }),
        )
    }

    fn emit_assign_place_with_continuation(
        &mut self,
        place: Expression,
        continuation: ExpressionContinuation,
    ) -> Vec<Statement> {
        match place {
            Expression::ArrayAccess(input) => {
                let ArrayAccess { array, index, span, id } = *input;
                self.emit_assign_place_with_continuation(
                    array,
                    Rc::new(move |array, this| {
                        let index = index.clone();
                        let continuation = continuation.clone();
                        this.emit_expression_with_continuation(
                            index,
                            Rc::new(move |index, this| {
                                let mut statements = Vec::new();
                                let index = this.materialize_strict_prefix_expression(index, &mut statements);
                                statements.extend(continuation.clone()(
                                    ArrayAccess { array: array.clone(), index, span, id }.into(),
                                    this,
                                ));
                                statements
                            }),
                        )
                    }),
                )
            }
            Expression::MemberAccess(input) => {
                let MemberAccess { inner, name, span, id } = *input;
                self.emit_assign_place_with_continuation(
                    inner,
                    Rc::new(move |inner, this| {
                        continuation.clone()(MemberAccess { inner, name, span, id }.into(), this)
                    }),
                )
            }
            Expression::TupleAccess(input) => {
                let TupleAccess { tuple, index, span, id } = *input;
                self.emit_assign_place_with_continuation(
                    tuple,
                    Rc::new(move |tuple, this| {
                        continuation.clone()(
                            TupleAccess { tuple: tuple.clone(), index: index.clone(), span, id }.into(),
                            this,
                        )
                    }),
                )
            }
            place => continuation(place, self),
        }
    }

    fn expression_requires_branch_cps(expression: &Expression) -> bool {
        match expression {
            Expression::Array(input) => input.elements.iter().any(Self::expression_requires_branch_cps),
            Expression::ArrayAccess(input) => {
                Self::expression_requires_branch_cps(&input.array) || Self::expression_requires_branch_cps(&input.index)
            }
            Expression::Binary(input) => {
                Self::expression_requires_branch_cps(&input.left) || Self::expression_requires_branch_cps(&input.right)
            }
            Expression::Call(input) => {
                input.const_arguments.iter().any(Self::expression_requires_branch_cps)
                    || input.arguments.iter().any(Self::expression_requires_branch_cps)
            }
            Expression::Cast(input) => Self::expression_requires_branch_cps(&input.expression),
            Expression::Composite(input) => {
                input.const_arguments.iter().any(Self::expression_requires_branch_cps)
                    || input
                        .members
                        .iter()
                        .filter_map(|member| member.expression.as_ref())
                        .any(Self::expression_requires_branch_cps)
                    || input.base.as_deref().is_some_and(Self::expression_requires_branch_cps)
            }
            Expression::DynamicOp(input) => {
                Self::expression_requires_branch_cps(&input.target_program)
                    || input.network.as_ref().is_some_and(Self::expression_requires_branch_cps)
                    || match &input.kind {
                        DynamicOpKind::Read { .. } | DynamicOpKind::Op { .. } => true,
                        DynamicOpKind::Call { arguments, .. } => {
                            arguments.iter().any(Self::expression_requires_branch_cps)
                        }
                    }
            }
            Expression::Intrinsic(input) => {
                input.arguments.iter().any(Self::expression_requires_branch_cps)
                    || matches!(
                        Intrinsic::from_symbol(input.name, &input.type_parameters),
                        Some(Intrinsic::VectorPop | Intrinsic::VectorGet)
                    )
            }
            Expression::MemberAccess(input) => Self::expression_requires_branch_cps(&input.inner),
            Expression::Repeat(input) => {
                Self::expression_requires_branch_cps(&input.expr) || Self::expression_requires_branch_cps(&input.count)
            }
            Expression::Ternary(input) => {
                Self::expression_requires_branch_cps(&input.condition)
                    || Self::expression_may_emit_cps_statements(&input.if_true)
                    || Self::expression_may_emit_cps_statements(&input.if_false)
            }
            Expression::Tuple(input) => input.elements.iter().any(Self::expression_requires_branch_cps),
            Expression::TupleAccess(input) => Self::expression_requires_branch_cps(&input.tuple),
            Expression::Unary(input) => Self::expression_requires_branch_cps(&input.receiver),
            _ => false,
        }
    }

    fn expression_requires_tail_cps(&self, expression: &Expression) -> bool {
        match expression {
            Expression::Array(input) => input.elements.iter().any(|element| self.expression_requires_tail_cps(element)),
            Expression::ArrayAccess(input) => {
                self.expression_requires_tail_cps(&input.array) || self.expression_requires_tail_cps(&input.index)
            }
            Expression::Binary(input) => {
                self.expression_requires_tail_cps(&input.left) || self.expression_requires_tail_cps(&input.right)
            }
            Expression::Call(input) => {
                input.const_arguments.iter().any(|argument| self.expression_requires_tail_cps(argument))
                    || input.arguments.iter().any(|argument| self.expression_requires_tail_cps(argument))
            }
            Expression::Cast(input) => self.expression_requires_tail_cps(&input.expression),
            Expression::Composite(input) => {
                input.const_arguments.iter().any(|argument| self.expression_requires_tail_cps(argument))
                    || input
                        .members
                        .iter()
                        .filter_map(|member| member.expression.as_ref())
                        .any(|expression| self.expression_requires_tail_cps(expression))
                    || input.base.as_deref().is_some_and(|base| self.expression_requires_tail_cps(base))
            }
            Expression::DynamicOp(input) => {
                self.expression_requires_tail_cps(&input.target_program)
                    || input.network.as_ref().is_some_and(|network| self.expression_requires_tail_cps(network))
                    || match &input.kind {
                        DynamicOpKind::Read { .. } | DynamicOpKind::Op { .. } => true,
                        DynamicOpKind::Call { arguments, .. } => {
                            arguments.iter().any(|argument| self.expression_requires_tail_cps(argument))
                        }
                    }
            }
            Expression::Intrinsic(input) => {
                Self::expression_requires_branch_cps(expression)
                    || input.arguments.iter().any(|argument| self.expression_requires_tail_cps(argument))
            }
            Expression::MemberAccess(input) => self.expression_requires_tail_cps(&input.inner),
            Expression::Repeat(input) => {
                self.expression_requires_tail_cps(&input.expr) || self.expression_requires_tail_cps(&input.count)
            }
            Expression::Ternary(input) => {
                self.expression_requires_tail_cps(&input.condition)
                    || self.expression_may_emit_after_reconstruction(&input.if_true)
                    || self.expression_may_emit_after_reconstruction(&input.if_false)
            }
            Expression::Tuple(input) => input.elements.iter().any(|element| self.expression_requires_tail_cps(element)),
            Expression::TupleAccess(input) => self.expression_requires_tail_cps(&input.tuple),
            Expression::Unary(input) => self.expression_requires_tail_cps(&input.receiver),
            _ => Self::expression_requires_branch_cps(expression),
        }
    }

    fn expression_may_emit_after_reconstruction(&self, expression: &Expression) -> bool {
        Self::expression_may_emit_cps_statements(expression) || self.expression_reconstructs_to_branch_cps(expression)
    }

    fn expression_reconstructs_to_branch_cps(&self, expression: &Expression) -> bool {
        match expression {
            Expression::Array(input) => {
                input.elements.iter().any(|element| self.expression_reconstructs_to_branch_cps(element))
            }
            Expression::ArrayAccess(input) => {
                self.expression_reconstructs_to_branch_cps(&input.array)
                    || self.expression_reconstructs_to_branch_cps(&input.index)
            }
            Expression::Binary(input) => {
                self.expression_reconstructs_to_branch_cps(&input.left)
                    || self.expression_reconstructs_to_branch_cps(&input.right)
            }
            Expression::Call(input) => {
                input.const_arguments.iter().any(|argument| self.expression_reconstructs_to_branch_cps(argument))
                    || input.arguments.iter().any(|argument| self.expression_reconstructs_to_branch_cps(argument))
            }
            Expression::Cast(input) => self.expression_reconstructs_to_branch_cps(&input.expression),
            Expression::Composite(input) => {
                input.const_arguments.iter().any(|argument| self.expression_reconstructs_to_branch_cps(argument))
                    || input
                        .members
                        .iter()
                        .filter_map(|member| member.expression.as_ref())
                        .any(|expression| self.expression_reconstructs_to_branch_cps(expression))
                    || input.base.as_deref().is_some_and(|base| self.expression_reconstructs_to_branch_cps(base))
            }
            Expression::DynamicOp(input) => {
                self.expression_reconstructs_to_branch_cps(&input.target_program)
                    || input.network.as_ref().is_some_and(|network| self.expression_reconstructs_to_branch_cps(network))
                    || match &input.kind {
                        DynamicOpKind::Read { .. } | DynamicOpKind::Op { .. } => false,
                        DynamicOpKind::Call { arguments, .. } => {
                            arguments.iter().any(|argument| self.expression_reconstructs_to_branch_cps(argument))
                        }
                    }
            }
            Expression::Intrinsic(input) => {
                input.arguments.iter().any(|argument| self.expression_reconstructs_to_branch_cps(argument))
            }
            Expression::MemberAccess(input) => self.expression_reconstructs_to_branch_cps(&input.inner),
            Expression::Path(path) => self.path_reconstructs_to_branch_cps(path),
            Expression::Repeat(input) => {
                self.expression_reconstructs_to_branch_cps(&input.expr)
                    || self.expression_reconstructs_to_branch_cps(&input.count)
            }
            Expression::Ternary(input) => {
                self.expression_reconstructs_to_branch_cps(&input.condition)
                    || self.expression_reconstructs_to_branch_cps(&input.if_true)
                    || self.expression_reconstructs_to_branch_cps(&input.if_false)
            }
            Expression::Tuple(input) => {
                input.elements.iter().any(|element| self.expression_reconstructs_to_branch_cps(element))
            }
            Expression::TupleAccess(input) => self.expression_reconstructs_to_branch_cps(&input.tuple),
            Expression::Unary(input) => self.expression_reconstructs_to_branch_cps(&input.receiver),
            _ => false,
        }
    }

    fn path_reconstructs_to_branch_cps(&self, path: &Path) -> bool {
        let Some(location) = path.try_global_location() else {
            return false;
        };
        self.state
            .symbol_table
            .lookup_global(self.program, location)
            .and_then(|var| var.type_.as_ref())
            .is_some_and(|type_| matches!(type_, Type::Optional(_)))
    }

    fn statement_requires_tail_cps(&self, statement: &Statement) -> bool {
        match statement {
            Statement::Assert(input) => match &input.variant {
                AssertVariant::Assert(expression) => self.expression_requires_tail_cps(expression),
                AssertVariant::AssertEq(left, right) | AssertVariant::AssertNeq(left, right) => {
                    self.expression_requires_tail_cps(left) || self.expression_requires_tail_cps(right)
                }
            },
            Statement::Assign(input) => {
                self.expression_requires_tail_cps(&input.place) || self.expression_requires_tail_cps(&input.value)
            }
            Statement::Block(_) => false,
            Statement::Conditional(input) => self.expression_requires_tail_cps(&input.condition),
            Statement::Const(input) => self.expression_requires_tail_cps(&input.value),
            Statement::Definition(input) => self.expression_requires_tail_cps(&input.value),
            Statement::Expression(input) => self.expression_requires_tail_cps(&input.expression),
            Statement::Iteration(input) => {
                self.expression_requires_tail_cps(&input.start) || self.expression_requires_tail_cps(&input.stop)
            }
            Statement::Return(input) => self.expression_requires_tail_cps(&input.expression),
        }
    }

    fn reconstruct_block_statements(&mut self, statements: Vec<Statement>) -> Vec<Statement> {
        let mut remaining = std::collections::VecDeque::from(statements);
        let mut reconstructed = Vec::new();

        while let Some(statement) = remaining.pop_front() {
            if self.statement_requires_tail_cps(&statement) {
                // Replay the remaining block inside the first branch-sensitive statement, so
                // statements after it only observe values from the branch that produced them.
                let tail = remaining.into_iter().collect::<Vec<_>>();
                reconstructed.extend(self.reconstruct_statement_with_tail(statement, &tail));
                return reconstructed;
            }

            let (statement, additional_statements) = self.reconstruct_statement(statement);
            reconstructed.extend(additional_statements);
            reconstructed.push(statement);
        }

        reconstructed
    }

    fn reconstruct_statement_with_tail(&mut self, statement: Statement, tail: &[Statement]) -> Vec<Statement> {
        match statement {
            Statement::Assert(input) => self.reconstruct_assert_with_tail(input, tail),
            Statement::Assign(input) => self.reconstruct_assign_with_tail(*input, tail),
            Statement::Block(input) => self.reconstruct_block_with_tail(input, tail),
            Statement::Conditional(input) => self.reconstruct_conditional_with_tail(input, tail),
            Statement::Const(input) => self.reconstruct_const_with_tail(input, tail),
            Statement::Definition(input) => self.reconstruct_definition_with_tail(input, tail),
            Statement::Expression(input) => self.reconstruct_expression_statement_with_tail(input, tail),
            Statement::Iteration(input) => self.reconstruct_iteration_with_tail(*input, tail),
            Statement::Return(input) => self.reconstruct_return_with_tail(input),
        }
    }

    fn statement_then_tail(&mut self, statement: Statement, tail: Vec<Statement>) -> Vec<Statement> {
        let mut statements = vec![statement];
        statements.extend(self.reconstruct_block_statements(tail));
        statements
    }

    fn emitted_statements_then_tail(&mut self, mut statements: Vec<Statement>, tail: &[Statement]) -> Vec<Statement> {
        statements.extend(self.reconstruct_block_statements(tail.to_vec()));
        statements
    }

    fn split_tail_after_last_local_use(
        &mut self,
        tail: &[Statement],
        symbol: Symbol,
    ) -> (Vec<Statement>, Vec<Statement>) {
        let mut branch_symbols = vec![symbol];
        let mut split_index = 0;
        for (index, statement) in tail.iter().enumerate() {
            if self.statement_uses_any_local_symbol(statement, &branch_symbols) {
                let old_split_index = split_index;
                split_index = index + 1;
                tail[old_split_index..split_index]
                    .iter()
                    .for_each(|statement| Self::extend_branch_local_symbols(statement, &mut branch_symbols));
            }
        }

        (tail[..split_index].to_vec(), tail[split_index..].to_vec())
    }

    fn statement_uses_any_local_symbol(&mut self, statement: &Statement, symbols: &[Symbol]) -> bool {
        let mut collector = SymbolAccessCollector::new(self.state);
        collector.visit_statement(statement);
        collector
            .symbol_accesses
            .iter()
            .any(|(path, _)| symbols.iter().any(|symbol| Self::path_matches_local_symbol(path, *symbol)))
    }

    fn path_matches_local_symbol(path: &Path, symbol: Symbol) -> bool {
        path.try_local_symbol().is_some_and(|local| local == symbol)
            || (path.identifier().name == symbol
                && path.try_global_location().is_none()
                && path.qualifier().is_empty()
                && path.program().is_none())
    }

    fn extend_branch_local_symbols(statement: &Statement, symbols: &mut Vec<Symbol>) {
        match statement {
            Statement::Assign(input) => {
                if let Some(symbol) = Self::assigned_local_symbol(&input.place) {
                    Self::push_branch_local_symbol(symbols, symbol);
                }
            }
            Statement::Const(input) => Self::push_branch_local_symbol(symbols, input.place.name),
            Statement::Definition(input) => match &input.place {
                DefinitionPlace::Single(identifier) => Self::push_branch_local_symbol(symbols, identifier.name),
                DefinitionPlace::Multiple(identifiers) => {
                    identifiers.iter().for_each(|identifier| Self::push_branch_local_symbol(symbols, identifier.name));
                }
            },
            _ => {}
        }
    }

    fn assigned_local_symbol(expression: &Expression) -> Option<Symbol> {
        match expression {
            Expression::ArrayAccess(input) => Self::assigned_local_symbol(&input.array),
            Expression::MemberAccess(input) => Self::assigned_local_symbol(&input.inner),
            Expression::Path(path) => path.try_local_symbol().or_else(|| {
                (path.try_global_location().is_none() && path.qualifier().is_empty() && path.program().is_none())
                    .then_some(path.identifier().name)
            }),
            Expression::TupleAccess(input) => Self::assigned_local_symbol(&input.tuple),
            _ => None,
        }
    }

    fn push_branch_local_symbol(symbols: &mut Vec<Symbol>, symbol: Symbol) {
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }

    fn reconstruct_block_with_tail(&mut self, input: Block, tail: &[Statement]) -> Vec<Statement> {
        let (block, additional_statements) = self.reconstruct_block(input);
        debug_assert!(additional_statements.is_empty(), "block reconstruction returns no unconditional prelude");
        self.statement_then_tail(block.into(), tail.to_vec())
    }

    fn reconstruct_const_with_tail(&mut self, input: ConstDeclaration, tail: &[Statement]) -> Vec<Statement> {
        let (type_, mut statements) = self.reconstruct_type(input.type_.clone());
        let tail = tail.to_vec();
        statements.extend(self.emit_expression_with_continuation(
            input.value.clone(),
            Rc::new(move |value, this| {
                let statement = ConstDeclaration { type_: type_.clone(), value, ..input.clone() }.into();
                this.statement_then_tail(statement, tail.clone())
            }),
        ));
        statements
    }

    fn reconstruct_definition_with_tail(&mut self, input: DefinitionStatement, tail: &[Statement]) -> Vec<Statement> {
        let type_ = input.type_.clone().map(|type_| self.reconstruct_type(type_).0);
        let tail = tail.to_vec();

        if let DefinitionPlace::Single(identifier) = input.place {
            let (branch_tail, remaining_tail) = self.split_tail_after_last_local_use(&tail, identifier.name);
            let statements = self.emit_expression_with_continuation(
                input.value.clone(),
                Rc::new(move |value, this| {
                    let statement = DefinitionStatement {
                        place: DefinitionPlace::Single(identifier),
                        type_: type_.clone(),
                        value,
                        span: input.span,
                        id: this.state.node_builder.next_id(),
                    }
                    .into();
                    this.statement_then_tail(statement, branch_tail.clone())
                }),
            );
            return self.emitted_statements_then_tail(statements, &remaining_tail);
        }

        self.emit_expression_with_continuation(
            input.value.clone(),
            Rc::new(move |value, this| {
                let statement = DefinitionStatement {
                    place: input.place.clone(),
                    type_: type_.clone(),
                    value,
                    span: input.span,
                    id: this.state.node_builder.next_id(),
                }
                .into();
                this.statement_then_tail(statement, tail.clone())
            }),
        )
    }

    fn reconstruct_assert_with_tail(&mut self, input: AssertStatement, tail: &[Statement]) -> Vec<Statement> {
        let tail = tail.to_vec();

        let statements = match input.variant.clone() {
            AssertVariant::Assert(expression) => self.emit_expression_with_continuation(
                expression,
                Rc::new(move |expression, this| {
                    let statement = AssertStatement {
                        variant: AssertVariant::Assert(expression),
                        span: input.span,
                        id: this.state.node_builder.next_id(),
                    }
                    .into();
                    vec![statement]
                }),
            ),
            AssertVariant::AssertEq(left, right) => self.emit_expression_with_continuation(
                left,
                Rc::new(move |left, this| {
                    let mut statements = Vec::new();
                    let right_may_emit = this.expression_may_emit_after_reconstruction(&right);
                    let left = if right_may_emit {
                        this.materialize_strict_prefix_expression(left, &mut statements)
                    } else {
                        left
                    };
                    let input = input.clone();
                    let right = right.clone();
                    statements.extend(this.emit_expression_with_continuation(
                        right,
                        Rc::new(move |right, this| {
                            let statement = AssertStatement {
                                variant: AssertVariant::AssertEq(left.clone(), right),
                                span: input.span,
                                id: this.state.node_builder.next_id(),
                            }
                            .into();
                            vec![statement]
                        }),
                    ));
                    statements
                }),
            ),
            AssertVariant::AssertNeq(left, right) => self.emit_expression_with_continuation(
                left,
                Rc::new(move |left, this| {
                    let mut statements = Vec::new();
                    let right_may_emit = this.expression_may_emit_after_reconstruction(&right);
                    let left = if right_may_emit {
                        this.materialize_strict_prefix_expression(left, &mut statements)
                    } else {
                        left
                    };
                    let input = input.clone();
                    let right = right.clone();
                    statements.extend(this.emit_expression_with_continuation(
                        right,
                        Rc::new(move |right, this| {
                            let statement = AssertStatement {
                                variant: AssertVariant::AssertNeq(left.clone(), right),
                                span: input.span,
                                id: this.state.node_builder.next_id(),
                            }
                            .into();
                            vec![statement]
                        }),
                    ));
                    statements
                }),
            ),
        };
        self.emitted_statements_then_tail(statements, &tail)
    }

    fn reconstruct_assign_with_tail(&mut self, input: AssignStatement, tail: &[Statement]) -> Vec<Statement> {
        let AssignStatement { place, value, span, .. } = input;
        let tail = tail.to_vec();

        if let Expression::Path(path) = &place
            && let Some(global_location) = path.try_global_location()
        {
            let var = self
                .state
                .symbol_table
                .lookup_global(self.program, global_location)
                .expect("A global path must point to a global");
            assert!(
                var.type_.as_ref().expect("must be known by now").is_optional(),
                "Only storage variables that are not vectors or mappings are expected here."
            );

            let var_name = path.identifier().name;
            let mapping_symbol = Symbol::intern(&format!("{var_name}__"));
            let statements = self.emit_expression_with_continuation(
                value,
                Rc::new(move |new_value, this| {
                    let mapping_ident = Identifier::new(mapping_symbol, this.state.node_builder.next_id());
                    let mapping_expr: Expression =
                        Path::from(mapping_ident).to_global(Location::new(this.program, vec![mapping_symbol])).into();
                    let false_literal: Expression =
                        Literal::boolean(false, Span::default(), this.state.node_builder.next_id()).into();
                    let statement =
                        if matches!(&new_value, Expression::Literal(Literal { variant: LiteralVariant::None, .. })) {
                            let remove_expr: Expression = IntrinsicExpression {
                                name: sym::_mapping_remove,
                                type_parameters: vec![],
                                input_types: vec![],
                                return_types: vec![],
                                arguments: vec![mapping_expr, false_literal],
                                span,
                                id: this.state.node_builder.next_id(),
                            }
                            .into();
                            Statement::Expression(ExpressionStatement {
                                expression: remove_expr,
                                span,
                                id: this.state.node_builder.next_id(),
                            })
                        } else {
                            let set_expr: Expression = IntrinsicExpression {
                                name: sym::_mapping_set,
                                type_parameters: vec![],
                                input_types: vec![],
                                return_types: vec![],
                                arguments: vec![mapping_expr, false_literal, new_value],
                                span,
                                id: this.state.node_builder.next_id(),
                            }
                            .into();
                            Statement::Expression(ExpressionStatement {
                                expression: set_expr,
                                span,
                                id: this.state.node_builder.next_id(),
                            })
                        };

                    vec![statement]
                }),
            );
            return self.emitted_statements_then_tail(statements, &tail);
        }

        let statements = self.emit_assign_place_with_continuation(
            place,
            Rc::new(move |place, this| {
                let value = value.clone();
                this.emit_expression_with_continuation(
                    value,
                    Rc::new(move |value, this| {
                        let statement = AssignStatement {
                            place: place.clone(),
                            value,
                            span,
                            id: this.state.node_builder.next_id(),
                        }
                        .into();
                        vec![statement]
                    }),
                )
            }),
        );
        self.emitted_statements_then_tail(statements, &tail)
    }

    fn reconstruct_expression_statement_with_tail(
        &mut self,
        input: ExpressionStatement,
        tail: &[Statement],
    ) -> Vec<Statement> {
        let keep_expression = !expression_can_be_discarded(&input.expression, self.state);
        let tail = tail.to_vec();

        let statements = self.emit_expression_with_continuation(
            input.expression.clone(),
            Rc::new(move |expression, this| {
                let legal_expression_statement =
                    matches!(expression, Expression::Call(_) | Expression::DynamicOp(_) | Expression::Intrinsic(_));
                let statement = if legal_expression_statement {
                    ExpressionStatement { expression, span: input.span, id: this.state.node_builder.next_id() }.into()
                } else if keep_expression {
                    let discard_sym = this.state.assigner.unique_symbol("$discard", "$");
                    let discard_ident =
                        Identifier { name: discard_sym, span: input.span, id: this.state.node_builder.next_id() };
                    if let Some(type_) = this.state.type_table.get(&expression.id()) {
                        this.state.type_table.insert(discard_ident.id, type_);
                    }
                    DefinitionStatement {
                        place: DefinitionPlace::Single(discard_ident),
                        type_: None,
                        value: expression,
                        span: input.span,
                        id: this.state.node_builder.next_id(),
                    }
                    .into()
                } else {
                    ExpressionStatement {
                        expression: UnitExpression { span: input.span, id: this.state.node_builder.next_id() }.into(),
                        span: input.span,
                        id: this.state.node_builder.next_id(),
                    }
                    .into()
                };
                vec![statement]
            }),
        );
        self.emitted_statements_then_tail(statements, &tail)
    }

    fn reconstruct_conditional_with_tail(&mut self, input: ConditionalStatement, tail: &[Statement]) -> Vec<Statement> {
        let tail = tail.to_vec();

        let statements = self.emit_expression_with_continuation(
            input.condition.clone(),
            Rc::new(move |condition, this| {
                let (then, then_statements) = this.reconstruct_block(input.then.clone());
                debug_assert!(then_statements.is_empty(), "block reconstruction returns no unconditional prelude");
                let otherwise =
                    input.otherwise.clone().map(|otherwise| this.reconstruct_otherwise_statement(*otherwise));
                let statement = ConditionalStatement {
                    condition,
                    then,
                    otherwise,
                    span: input.span,
                    id: this.state.node_builder.next_id(),
                }
                .into();
                vec![statement]
            }),
        );
        self.emitted_statements_then_tail(statements, &tail)
    }

    fn reconstruct_otherwise_statement(&mut self, statement: Statement) -> Box<Statement> {
        let mut statements = self.reconstruct_statement_with_tail(statement, &[]);
        if statements.len() == 1 {
            Box::new(statements.pop().unwrap())
        } else {
            let span = statements
                .iter()
                .map(|statement| statement.span())
                .reduce(|left, right| left + right)
                .unwrap_or_default();
            Box::new(Block { statements, span, id: self.state.node_builder.next_id() }.into())
        }
    }

    fn reconstruct_iteration_with_tail(&mut self, input: IterationStatement, tail: &[Statement]) -> Vec<Statement> {
        let type_ = input.type_.clone().map(|type_| self.reconstruct_type(type_).0);
        let tail = tail.to_vec();

        let statements = self.emit_expression_with_continuation(
            input.start.clone(),
            Rc::new(move |start, this| {
                let mut statements = Vec::new();
                let start = this.materialize_strict_prefix_expression(start, &mut statements);
                let input = input.clone();
                let type_ = type_.clone();
                statements.extend(this.emit_expression_with_continuation(
                    input.stop.clone(),
                    Rc::new(move |stop, this| {
                        let (block, block_statements) = this.reconstruct_block(input.block.clone());
                        debug_assert!(
                            block_statements.is_empty(),
                            "block reconstruction returns no unconditional prelude"
                        );
                        let statement = IterationStatement {
                            variable: input.variable,
                            type_: type_.clone(),
                            start: start.clone(),
                            stop,
                            inclusive: input.inclusive,
                            block,
                            span: input.span,
                            id: this.state.node_builder.next_id(),
                        }
                        .into();
                        vec![statement]
                    }),
                ));
                statements
            }),
        );
        self.emitted_statements_then_tail(statements, &tail)
    }

    fn reconstruct_return_with_tail(&mut self, input: ReturnStatement) -> Vec<Statement> {
        self.emit_expression_with_continuation(
            input.expression.clone(),
            Rc::new(move |expression, this| {
                vec![ReturnStatement { expression, span: input.span, id: this.state.node_builder.next_id() }.into()]
            }),
        )
    }

    fn split_emitted_statement(&mut self, mut statements: Vec<Statement>) -> (Statement, Vec<Statement>) {
        let statement = statements.pop().expect("statement reconstruction should emit at least one statement");
        (statement, statements)
    }
}
impl leo_ast::AstReconstructor for StorageLoweringVisitor<'_> {
    type AdditionalInput = ();
    type AdditionalOutput = Vec<Statement>;

    /* Types */
    fn reconstruct_array_type(&mut self, input: ArrayType) -> (Type, Self::AdditionalOutput) {
        let (length, stmts) = self.reconstruct_expression(*input.length, &());
        (
            Type::Array(ArrayType {
                element_type: Box::new(self.reconstruct_type(*input.element_type).0),
                length: Box::new(length),
            }),
            stmts,
        )
    }

    fn reconstruct_composite_type(&mut self, input: CompositeType) -> (Type, Self::AdditionalOutput) {
        let mut statements = Vec::new();

        let const_arguments = input
            .const_arguments
            .into_iter()
            .map(|arg| {
                let (expr, stmts) = self.reconstruct_expression(arg, &Default::default());
                statements.extend(stmts);
                expr
            })
            .collect();

        (Type::Composite(CompositeType { const_arguments, ..input }), statements)
    }

    /* Expressions */
    fn reconstruct_array_access(
        &mut self,
        mut input: ArrayAccess,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        let (array, mut stmts_array) = self.reconstruct_expression(input.array, &());
        let (index, mut stmts_index) = self.reconstruct_expression(input.index, &());

        input.array = array;
        input.index = index;

        // Merge side effects
        stmts_array.append(&mut stmts_index);

        (input.into(), stmts_array)
    }

    fn reconstruct_intrinsic(
        &mut self,
        mut input: IntrinsicExpression,
        _additional: &Self::AdditionalInput,
    ) -> (Expression, Self::AdditionalOutput) {
        match Intrinsic::from_symbol(input.name, &input.type_parameters) {
            Some(Intrinsic::VectorPush) => {
                // Unpack arguments
                let [vector_expr, value_expr] = &mut input.arguments[..] else {
                    panic!("Vector::push should have 2 arguments");
                };

                // Validate vector type
                assert!(matches!(self.state.type_table.get(&vector_expr.id()), Some(Type::Vector(_))));
                let Expression::Path(path_to_vector) = vector_expr else {
                    panic!("Vector::push can only be called with `Expression::Path`");
                };

                let value_type = self
                    .state
                    .type_table
                    .get(&value_expr.id())
                    .expect("type checking should assign a type to the pushed value");
                let value_must_be_evaluated_first = !expression_can_be_discarded(value_expr, self.state);
                let (value, mut stmts) = self.reconstruct_expression(value_expr.clone(), &());
                self.state.type_table.insert(value.id(), value_type.clone());

                // Input:
                //   Vector::push(v, value)
                //
                // Lowered reconstruction:
                //   let $push_value = value; // if the value cannot be discarded
                //   let $len_var = Mapping::get_or_use(len_map, false, 0u32);
                //   Mapping::set(len_map, false, $len_var + 1);
                //   Mapping::set(vec_map, $len_var, value or $push_value);
                //
                // If the pushed value cannot be discarded, bind it before mutating
                // vector length so source argument evaluation order is preserved.
                let value = if value_must_be_evaluated_first {
                    let value_var_sym = self.state.assigner.unique_symbol("$push_value", "$");
                    let value_var_ident = Identifier {
                        name: value_var_sym,
                        span: Default::default(),
                        id: self.state.node_builder.next_id(),
                    };
                    self.state.type_table.insert(value_var_ident.id, value_type);
                    stmts.push(self.state.assigner.simple_definition(
                        value_var_ident,
                        value,
                        self.state.node_builder.next_id(),
                    ));
                    Path::from(value_var_ident).to_local().into()
                } else {
                    value
                };

                // Reconstruct the backing mappings.
                let (vec_path_expr, len_path_expr) = self.generate_vector_mapping_exprs(path_to_vector);

                // let $len_var = Mapping::get_or_use(len_map, false, 0u32)
                let len_var_sym = self.state.assigner.unique_symbol("$len_var", "$");
                let len_var_ident =
                    Identifier { name: len_var_sym, span: Default::default(), id: self.state.node_builder.next_id() };
                let get_len_expr = self.get_vector_len_expr(len_path_expr.clone(), input.span);
                let len_stmt = self.state.assigner.simple_definition(
                    len_var_ident,
                    get_len_expr,
                    self.state.node_builder.next_id(),
                );
                let len_var_expr: Expression = Path::from(len_var_ident).to_local().into();

                // index + 1
                let literal_one = self.literal_one_u32();
                let increment_expr = self.binary_expr(len_var_expr.clone(), BinaryOperation::Add, literal_one);

                // Mapping::set(vec__, $len_var, value)
                let set_vec_stmt_expr = self.set_mapping_expr(vec_path_expr, len_var_expr.clone(), value, input.span);

                // Mapping::set(len_map, false, $len_var + 1)
                let literal_false = self.literal_false();
                let set_len_stmt = Statement::Expression(ExpressionStatement {
                    expression: self.set_mapping_expr(len_path_expr, literal_false, increment_expr, input.span),
                    span: input.span,
                    id: self.state.node_builder.next_id(),
                });

                (set_vec_stmt_expr, [stmts, vec![len_stmt, set_len_stmt]].concat())
            }

            Some(Intrinsic::VectorLen) => {
                // Input:
                //   Vector::len(v)
                //
                // Lowered reconstruction:
                //   Mapping::get_or_use(len_map, false, 0u32)

                //  Unpack arguments
                let [vector_expr] = &mut input.arguments[..] else {
                    panic!("Vector::len should have 1 argument");
                };

                // Validate vector type
                assert!(matches!(self.state.type_table.get(&vector_expr.id()), Some(Type::Vector(_))));
                let Expression::Path(path_to_vector) = vector_expr else {
                    panic!("Vector::len can only be called with `Expression::Path`");
                };

                let (_vec_path_expr, len_path_expr) = self.generate_vector_mapping_exprs(path_to_vector);

                let get_len_expr = self.get_vector_len_expr(len_path_expr, input.span);
                (get_len_expr, vec![])
            }

            Some(Intrinsic::VectorPop) => {
                // Unpack argument
                let [vector_expr] = &mut input.arguments[..] else {
                    panic!("Vector::pop should have 1 argument");
                };

                // Validate vector type
                let Some(Type::Vector(VectorType { element_type })) = self.state.type_table.get(&vector_expr.id())
                else {
                    panic!("argument to Vector::pop should be of type `Vector`.");
                };
                let Expression::Path(path_to_vector) = vector_expr else {
                    panic!("Vector::pop can only be called with `Expression::Path`");
                };

                // Input:
                //   Vector::pop(v)
                //
                // Lowered reconstruction:
                //   let $len_var = Mapping::get_or_use(len_map, false, 0u32);
                //   Mapping::set(len_map, false, $len_var > 0 ? $len_var - 1 : $len_var);
                //   $len_var > 0 ? Mapping::get_or_use(vec_map, $len_var - 1, zero_value) : None
                let (vec_path_expr, len_path_expr) = self.generate_vector_mapping_exprs(path_to_vector);

                // let $len_var = Mapping::get_or_use(len_map, false, 0u32)
                let len_var_sym = self.state.assigner.unique_symbol("$len_var", "$");
                let len_var_ident =
                    Identifier { name: len_var_sym, span: Default::default(), id: self.state.node_builder.next_id() };
                let get_len_expr = self.get_vector_len_expr(len_path_expr.clone(), input.span);
                let len_stmt = self.state.assigner.simple_definition(
                    len_var_ident,
                    get_len_expr,
                    self.state.node_builder.next_id(),
                );
                let len_var_expr: Expression = Path::from(len_var_ident).to_local().into();

                // $len_var > 0
                let literal_zero = self.literal_zero_u32();
                let len_gt_zero_expr = self.binary_expr(len_var_expr.clone(), BinaryOperation::Gt, literal_zero);

                // $len_var - 1
                let literal_one = self.literal_one_u32();
                let len_minus_one_expr =
                    self.binary_expr(len_var_expr.clone(), BinaryOperation::SubWrapped, literal_one);

                // ternary for new length: ($len_var > 0 ? $len_var - 1 : $len_var)
                let new_len_expr = self.ternary_expr(
                    len_gt_zero_expr.clone(),
                    len_minus_one_expr.clone(),
                    len_var_expr.clone(),
                    input.span,
                );

                // Mapping::set(len_map, false, new_len)
                let literal_false = self.literal_false();
                let set_len_stmt = Statement::Expression(ExpressionStatement {
                    expression: self.set_mapping_expr(len_path_expr.clone(), literal_false, new_len_expr, input.span),
                    span: input.span,
                    id: self.state.node_builder.next_id(),
                });

                // zero value for element type (used as default in get_or_use)
                let zero = self.zero(&element_type);

                // Mapping::get_or_use(vec_map, $len_var - 1, zero)
                let get_or_use_expr =
                    self.get_or_use_mapping_expr(vec_path_expr, len_minus_one_expr.clone(), zero, input.span);

                let none_expr = self.literal_none_optional(*element_type.clone());
                let ternary_expr = self.ternary_expr(len_gt_zero_expr, get_or_use_expr, none_expr, input.span);

                (ternary_expr, vec![len_stmt, set_len_stmt])
            }

            Some(Intrinsic::VectorGet) => {
                // Unpack arguments (container, index/key)
                let [vector_expr, key_expr] = &mut input.arguments[..] else {
                    panic!("Vector::get should have 2 arguments");
                };

                // Validate vector type
                let Some(Type::Vector(VectorType { element_type })) = self.state.type_table.get(&vector_expr.id())
                else {
                    panic!("argument to Vector::get should be of type `Vector`.");
                };
                let Expression::Path(path_to_vector) = vector_expr else {
                    panic!("Vector::get can only be called with `Expression::Path`");
                };

                // Reconstruct key/index.
                let key_type = self
                    .state
                    .type_table
                    .get(&key_expr.id())
                    .expect("type checking should assign a type to the vector index");
                let key_must_be_evaluated_once = !expression_can_be_discarded(key_expr, self.state);
                let (reconstructed_key_expr, mut key_stmts) =
                    self.reconstruct_expression(key_expr.clone(), &Default::default());
                self.state.type_table.insert(reconstructed_key_expr.id(), key_type.clone());
                let reconstructed_key_expr = if key_must_be_evaluated_once {
                    let key_var_sym = self.state.assigner.unique_symbol("$index", "$");
                    let key_var_ident = Identifier {
                        name: key_var_sym,
                        span: Default::default(),
                        id: self.state.node_builder.next_id(),
                    };
                    self.state.type_table.insert(key_var_ident.id, key_type);
                    key_stmts.push(self.state.assigner.simple_definition(
                        key_var_ident,
                        reconstructed_key_expr,
                        self.state.node_builder.next_id(),
                    ));
                    Path::from(key_var_ident).to_local().into()
                } else {
                    reconstructed_key_expr
                };

                // Input:
                //   Get(v, index)
                //
                // Lowered reconstruction:
                //   let $len_var = Mapping::get_or_use(len_map, false, 0u32);
                //   index < $len_var
                //       ? Mapping::get_or_use(vec_map, index, zero_value)
                //       : none
                let (vec_path_expr, len_path_expr) = self.generate_vector_mapping_exprs(path_to_vector);

                // let $len_var = Mapping::get_or_use(len_map, false, 0u32)
                let len_var_sym = self.state.assigner.unique_symbol("$len_var", "$");
                let len_var_ident =
                    Identifier { name: len_var_sym, span: Default::default(), id: self.state.node_builder.next_id() };
                let get_len_expr = self.get_vector_len_expr(len_path_expr.clone(), input.span);
                let len_stmt = self.state.assigner.simple_definition(
                    len_var_ident,
                    get_len_expr,
                    self.state.node_builder.next_id(),
                );
                let len_var_expr: Expression = Path::from(len_var_ident).to_local().into();

                // index < len
                let index_lt_len_expr =
                    self.binary_expr(reconstructed_key_expr.clone(), BinaryOperation::Lt, len_var_expr.clone());

                // zero value for element type (used as default in get_or_use)
                let zero = self.zero(&element_type);

                // Mapping::get(vec_map, index)
                let get_or_use_expr =
                    self.get_or_use_mapping_expr(vec_path_expr, reconstructed_key_expr.clone(), zero, input.span);

                let none_expr = self.literal_none_optional(*element_type.clone());
                let ternary_expr = self.ternary_expr(index_lt_len_expr, get_or_use_expr, none_expr, input.span);

                (ternary_expr, [key_stmts, vec![len_stmt]].concat())
            }

            Some(Intrinsic::VectorSet) => {
                // Unpack arguments (container, index/key, value)
                let [vector_expr, index_expr, value_expr] = &mut input.arguments[..] else {
                    panic!("Vector::set should have 3 arguments");
                };

                // Validate vector type
                assert!(
                    matches!(self.state.type_table.get(&vector_expr.id()), Some(Type::Vector(_))),
                    "argument to Vector::set should be of type `Vector`."
                );
                let Expression::Path(path_to_vector) = vector_expr else {
                    panic!("Vector::set can only be called with `Expression::Path`");
                };

                // Reconstruct key/index and value. Bind required residual evaluations in
                // source order before the vector operation's synthesized storage work.
                let (reconstructed_key_expr, mut key_stmts) =
                    self.reconstruct_expression(index_expr.clone(), &Default::default());
                let reconstructed_key_expr =
                    self.materialize_strict_prefix_expression(reconstructed_key_expr, &mut key_stmts);

                let (reconstructed_value_expr, mut value_stmts) =
                    self.reconstruct_expression(value_expr.clone(), &Default::default());
                let reconstructed_value_expr =
                    self.materialize_strict_prefix_expression(reconstructed_value_expr, &mut value_stmts);

                // Input:
                //   Set(v, index, value)
                //
                // Lowered reconstruction (conceptually):
                //   let $index = index; // if the residual index cannot be discarded
                //   let $set_value = value; // if the residual value cannot be discarded
                //   let $len_var = Mapping::get_or_use(len_map, false, 0u32);
                //   assert((index or $index) < $len_var);
                //   Mapping::set(vec_map, index or $index, value or $set_value);
                let (vec_path_expr, len_path_expr) = self.generate_vector_mapping_exprs(path_to_vector);

                // let $len_var = Mapping::get_or_use(len_map, false, 0u32)
                let len_var_sym = self.state.assigner.unique_symbol("$len_var", "$");
                let len_var_ident =
                    Identifier { name: len_var_sym, span: Default::default(), id: self.state.node_builder.next_id() };
                let get_len_expr = self.get_vector_len_expr(len_path_expr.clone(), input.span);
                let len_stmt = self.state.assigner.simple_definition(
                    len_var_ident,
                    get_len_expr,
                    self.state.node_builder.next_id(),
                );
                let len_var_expr: Expression = Path::from(len_var_ident).to_local().into();

                // index < $len_var
                let index_lt_len_expr =
                    self.binary_expr(reconstructed_key_expr.clone(), BinaryOperation::Lt, len_var_expr.clone());

                // Mapping::set(vec_map, index, value)
                let set_stmt_expr = self.set_mapping_expr(
                    vec_path_expr.clone(),
                    reconstructed_key_expr.clone(),
                    reconstructed_value_expr.clone(),
                    input.span,
                );

                // assert(index < len)
                let assert_stmt = Statement::Assert(AssertStatement {
                    variant: AssertVariant::Assert(index_lt_len_expr.clone()),
                    span: Span::default(),
                    id: self.state.node_builder.next_id(),
                });

                // Emit assert then set
                (set_stmt_expr, [key_stmts, value_stmts, vec![len_stmt, assert_stmt]].concat())
            }

            Some(Intrinsic::VectorClear) => {
                // Unpack arguments
                let [vector_expr] = &mut input.arguments[..] else {
                    panic!("Vector::clear should have 1 argument");
                };

                // Validate vector type
                assert!(
                    matches!(self.state.type_table.get(&vector_expr.id()), Some(Type::Vector(_))),
                    "argument to Vector::clear should be of type `Vector`."
                );
                let Expression::Path(path_to_vector) = vector_expr else {
                    panic!("Vector::clear can only be called with `Expression::Path`");
                };

                // Input:
                //   Vector::clear(v)
                //
                // Lowered reconstruction (conceptually):
                //   Mapping::set(len_map, false, 0u32);
                //
                // Note: `VectorClear` does not actually remove any elements from the mapping of
                // vector values.
                let (_vec_path_expr, len_path_expr) = self.generate_vector_mapping_exprs(path_to_vector);

                // Mapping::set(len_map, false, 0u32)
                let literal_false = self.literal_false();
                let literal_zero = self.literal_zero_u32();
                let set_len_stmt_expr = self.set_mapping_expr(len_path_expr, literal_false, literal_zero, input.span);

                (set_len_stmt_expr, vec![])
            }

            Some(Intrinsic::VectorSwapRemove) => {
                // Unpack arguments
                let [vector_expr, index_expr] = &mut input.arguments[..] else {
                    panic!("Vector::swap_remove should have 2 arguments");
                };

                // Validate vector type
                assert!(
                    matches!(self.state.type_table.get(&vector_expr.id()), Some(Type::Vector(_))),
                    "argument to Vector::swap_remove should be of type `Vector`."
                );
                let Expression::Path(path_to_vector) = vector_expr else {
                    panic!("Vector::swap_remove can only be called with `Expression::Path`");
                };

                // Reconstruct the index and bind a required residual evaluation once
                // before the vector operation's synthesized storage work.
                let (reconstructed_index_expr, mut index_stmts) =
                    self.reconstruct_expression(index_expr.clone(), &Default::default());
                let reconstructed_index_expr =
                    self.materialize_strict_prefix_expression(reconstructed_index_expr, &mut index_stmts);

                // Input:
                //   Vector::swap_remove(v, index)
                //
                // Lowered reconstruction (conceptually):
                //   let $index = index; // if the residual index cannot be discarded
                //   let $len_var = Mapping::get_or_use(len_map, false, 0u32);
                //   assert((index or $index) < $len_var);
                //   let $removed = Mapping::get(vec_map, index or $index);
                //   Mapping::set(vec_map, index or $index, Mapping::get(vec_map, $len_var - 1));
                //   Mapping::set(len_map, false, $len_var - 1);
                //   $removed
                let (vec_path_expr, len_path_expr) = self.generate_vector_mapping_exprs(path_to_vector);

                // let $len_var = Mapping::get_or_use(len_map, false, 0u32)
                let len_var_sym = self.state.assigner.unique_symbol("$len_var", "$");
                let len_var_ident =
                    Identifier { name: len_var_sym, span: Default::default(), id: self.state.node_builder.next_id() };
                let get_len_expr = self.get_vector_len_expr(len_path_expr.clone(), input.span);
                let len_stmt = self.state.assigner.simple_definition(
                    len_var_ident,
                    get_len_expr,
                    self.state.node_builder.next_id(),
                );
                let len_var_expr: Expression = Path::from(len_var_ident).to_local().into();

                // assert(index < $len_var);
                let index_lt_len_expr =
                    self.binary_expr(reconstructed_index_expr.clone(), BinaryOperation::Lt, len_var_expr.clone());
                let assert_stmt = Statement::Assert(AssertStatement {
                    variant: AssertVariant::Assert(index_lt_len_expr.clone()),
                    span: input.span,
                    id: self.state.node_builder.next_id(),
                });

                // let $removed = Mapping::get(vec_map, index); // the element to return
                let get_elem_expr =
                    self.get_mapping_expr(vec_path_expr.clone(), reconstructed_index_expr.clone(), input.span);
                let removed_sym = self.state.assigner.unique_symbol("$removed", "$");
                let removed_ident =
                    Identifier { name: removed_sym, span: Default::default(), id: self.state.node_builder.next_id() };
                let removed_stmt = Statement::Definition(DefinitionStatement {
                    place: DefinitionPlace::Single(removed_ident),
                    type_: None,
                    value: get_elem_expr,
                    span: input.span,
                    id: self.state.node_builder.next_id(),
                });

                // len - 1
                let literal_one = self.literal_one_u32();
                let len_minus_one_expr = self.binary_expr(len_var_expr.clone(), BinaryOperation::Sub, literal_one);

                // Mapping::set(vec_map, index, Mapping::get(vec_map, len - 1));
                let get_last_expr =
                    self.get_mapping_expr(vec_path_expr.clone(), len_minus_one_expr.clone(), input.span);
                let set_swap_stmt = Statement::Expression(ExpressionStatement {
                    expression: self.set_mapping_expr(
                        vec_path_expr.clone(),
                        reconstructed_index_expr.clone(),
                        get_last_expr,
                        input.span,
                    ),
                    span: input.span,
                    id: self.state.node_builder.next_id(),
                });

                // Mapping::set(len_map, false, len - 1);
                let literal_false = self.literal_false();
                let set_len_stmt = Statement::Expression(ExpressionStatement {
                    expression: self.set_mapping_expr(
                        len_path_expr.clone(),
                        literal_false,
                        len_minus_one_expr,
                        input.span,
                    ),
                    span: input.span,
                    id: self.state.node_builder.next_id(),
                });

                // Return `$removed` as the resulting expression
                (
                    Path::from(removed_ident).to_local().into(),
                    [index_stmts, vec![len_stmt, assert_stmt, removed_stmt, set_swap_stmt, set_len_stmt]].concat(),
                )
            }

            _ => {
                // Default: reconstruct all arguments, type parameters, input types, and return
                // types recursively and return the (possibly updated) original call.
                input.type_parameters =
                    input.type_parameters.into_iter().map(|(ty, span)| (self.reconstruct_type(ty).0, span)).collect();
                input.input_types = input
                    .input_types
                    .into_iter()
                    .map(|(mode, ty, span)| (mode, self.reconstruct_type(ty).0, span))
                    .collect();
                input.return_types = input
                    .return_types
                    .into_iter()
                    .map(|(mode, ty, span)| (mode, self.reconstruct_type(ty).0, span))
                    .collect();
                let statements: Vec<_> = input
                    .arguments
                    .iter_mut()
                    .flat_map(|arg| {
                        let (expr, stmts) = self.reconstruct_expression(std::mem::take(arg), &());
                        *arg = expr;
                        stmts
                    })
                    .collect();

                (input.into(), statements)
            }
        }
    }

    fn reconstruct_member_access(
        &mut self,
        mut input: MemberAccess,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        let (inner, stmts_inner) = self.reconstruct_expression(input.inner, &());

        input.inner = inner;

        (input.into(), stmts_inner)
    }

    fn reconstruct_repeat(
        &mut self,
        mut input: RepeatExpression,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        // Use expected type (if available) for `expr`
        let (expr, mut stmts_expr) = self.reconstruct_expression(input.expr, &());
        let (count, mut stmts_count) = self.reconstruct_expression(input.count, &());

        input.expr = expr;
        input.count = count;

        stmts_expr.append(&mut stmts_count);

        (input.into(), stmts_expr)
    }

    fn reconstruct_tuple_access(
        &mut self,
        mut input: TupleAccess,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        let (tuple, stmts) = self.reconstruct_expression(input.tuple, &());

        input.tuple = tuple;

        (input.into(), stmts)
    }

    fn reconstruct_array(
        &mut self,
        mut input: ArrayExpression,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        let mut all_stmts = Vec::new();
        let mut new_elements = Vec::with_capacity(input.elements.len());

        for element in input.elements.into_iter() {
            let (expr, mut stmts) = self.reconstruct_expression(element, &());
            all_stmts.append(&mut stmts);
            new_elements.push(expr);
        }

        input.elements = new_elements;

        (input.into(), all_stmts)
    }

    fn reconstruct_binary(
        &mut self,
        mut input: BinaryExpression,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        let (left, mut stmts_left) = self.reconstruct_expression(input.left, &());
        let (right, mut stmts_right) = self.reconstruct_expression(input.right, &());

        input.left = left;
        input.right = right;

        // Merge side effects
        stmts_left.append(&mut stmts_right);

        (input.into(), stmts_left)
    }

    fn reconstruct_call(&mut self, mut input: CallExpression, _addiional: &()) -> (Expression, Self::AdditionalOutput) {
        let mut statements = Vec::new();
        for arg in input.arguments.iter_mut() {
            let (expr, statements2) = self.reconstruct_expression(std::mem::take(arg), &());
            statements.extend(statements2);
            *arg = expr;
        }
        (input.into(), statements)
    }

    fn reconstruct_dynamic_op(
        &mut self,
        mut input: DynamicOpExpression,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        match input.kind {
            // Bare storage read: `Interface@(target)::name` → ternary over contains/get_or_use.
            DynamicOpKind::Read { storage } => {
                self.lower_dynamic_read(input.interface, input.target_program, input.network, storage, input.span)
            }

            // Storage member op: vector `.get(i)` and `.len()` are lowered here so that codegen
            // only ever sees mapping ops. Mapping ops are left in place, recursing into
            // subexpressions so nested lowering applies.
            DynamicOpKind::Op { member, op, arguments } => {
                let interface = self.lookup_interface_from_type(&input.interface);
                let vector_storage = interface
                    .storages
                    .iter()
                    .find(|s| s.identifier.name == member.name && matches!(s.type_, Type::Vector(_)))
                    .cloned();

                if let Some(storage_proto) = vector_storage {
                    let Type::Vector(VectorType { element_type }) = storage_proto.type_ else {
                        unreachable!("filtered above");
                    };
                    if op.name == sym::get {
                        return self.lower_dynamic_vector_get(
                            input.target_program,
                            input.network,
                            member,
                            *element_type,
                            arguments,
                            input.span,
                        );
                    }
                    debug_assert_eq!(op.name, sym::len, "type checking guarantees vector ops are `get` or `len`");
                    return self.lower_dynamic_vector_len(input.target_program, input.network, member, input.span);
                }

                // Mapping op — keep the DynamicOp shape, recurse into subexpressions.
                let mut stmts = Vec::new();
                let (tp, s) = self.reconstruct_expression(input.target_program, &());
                stmts.extend(s);
                input.target_program = tp;
                if let Some(n) = input.network {
                    let (ne, s) = self.reconstruct_expression(n, &());
                    stmts.extend(s);
                    input.network = Some(ne);
                }
                let new_arguments = arguments
                    .into_iter()
                    .map(|arg| {
                        let (e, s) = self.reconstruct_expression(arg, &());
                        stmts.extend(s);
                        e
                    })
                    .collect();
                input.kind = DynamicOpKind::Op { member, op, arguments: new_arguments };
                (input.into(), stmts)
            }

            // Function call: leave in place, recurse into subexpressions.
            DynamicOpKind::Call { function, arguments } => {
                let mut stmts = Vec::new();
                let (tp, s) = self.reconstruct_expression(input.target_program, &());
                stmts.extend(s);
                input.target_program = tp;
                if let Some(n) = input.network {
                    let (ne, s) = self.reconstruct_expression(n, &());
                    stmts.extend(s);
                    input.network = Some(ne);
                }
                let new_arguments = arguments
                    .into_iter()
                    .map(|arg| {
                        let (e, s) = self.reconstruct_expression(arg, &());
                        stmts.extend(s);
                        e
                    })
                    .collect();
                input.kind = DynamicOpKind::Call { function, arguments: new_arguments };
                (input.into(), stmts)
            }
        }
    }

    fn reconstruct_cast(&mut self, input: CastExpression, _addiional: &()) -> (Expression, Self::AdditionalOutput) {
        let (expression, statements) = self.reconstruct_expression(input.expression, &());
        (CastExpression { expression, ..input }.into(), statements)
    }

    fn reconstruct_composite_init(
        &mut self,
        mut input: CompositeExpression,
        _additional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        let mut statements = Vec::new();

        // Reconstruct const_arguments and extract statements
        for const_arg in input.const_arguments.iter_mut() {
            let (expr, statements2) = self.reconstruct_expression(const_arg.clone(), &());
            statements.extend(statements2);
            *const_arg = expr;
        }

        // Reconstruct members and extract statements
        for member in input.members.iter_mut() {
            assert!(member.expression.is_some());
            let (expr, statements2) = self.reconstruct_expression(member.expression.take().unwrap(), &());
            statements.extend(statements2);
            member.expression = Some(expr);
        }

        // Reconstruct the struct update base, if any.
        if let Some(base) = input.base.take() {
            let (expr, statements2) = self.reconstruct_expression(*base, &());
            statements.extend(statements2);
            input.base = Some(Box::new(expr));
        }

        (input.into(), statements)
    }

    fn reconstruct_path(&mut self, input: Path, _additional: &()) -> (Expression, Self::AdditionalOutput) {
        self.reconstruct_path_or_locator(input.into())
    }

    fn reconstruct_ternary(
        &mut self,
        input: TernaryExpression,
        _addiional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        // This legacy expression reconstructor can only return an unconditional prelude. Statement
        // owner contexts use `emit_expression_with_continuation`, whose ternary rule keeps arm-local
        // effects and consumers inside the selected branch.
        let (condition, mut statements) = self.reconstruct_expression(input.condition, &());
        let (if_true, statements2) = self.reconstruct_expression(input.if_true, &());
        let (if_false, statements3) = self.reconstruct_expression(input.if_false, &());
        statements.extend(statements2);
        statements.extend(statements3);
        (TernaryExpression { condition, if_true, if_false, ..input }.into(), statements)
    }

    fn reconstruct_tuple(
        &mut self,
        input: leo_ast::TupleExpression,
        _addiional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        // This should ony appear in a return statement.
        let mut statements = Vec::new();
        let elements = input
            .elements
            .into_iter()
            .map(|element| {
                let (expr, statements2) = self.reconstruct_expression(element, &());
                statements.extend(statements2);
                expr
            })
            .collect();
        (TupleExpression { elements, ..input }.into(), statements)
    }

    fn reconstruct_unary(
        &mut self,
        input: leo_ast::UnaryExpression,
        _addiional: &(),
    ) -> (Expression, Self::AdditionalOutput) {
        let (receiver, statements) = self.reconstruct_expression(input.receiver, &());
        (UnaryExpression { receiver, ..input }.into(), statements)
    }

    /* Statements */
    fn reconstruct_assert(&mut self, input: leo_ast::AssertStatement) -> (Statement, Self::AdditionalOutput) {
        let mut statements = Vec::new();
        let stmt = AssertStatement {
            variant: match input.variant {
                AssertVariant::Assert(expr) => {
                    let (expr, statements2) = self.reconstruct_expression(expr, &());
                    statements.extend(statements2);
                    AssertVariant::Assert(expr)
                }
                AssertVariant::AssertEq(left, right) => {
                    let (left, statements2) = self.reconstruct_expression(left, &());
                    statements.extend(statements2);
                    let (right, statements3) = self.reconstruct_expression(right, &());
                    statements.extend(statements3);
                    AssertVariant::AssertEq(left, right)
                }
                AssertVariant::AssertNeq(left, right) => {
                    let (left, statements2) = self.reconstruct_expression(left, &());
                    statements.extend(statements2);
                    let (right, statements3) = self.reconstruct_expression(right, &());
                    statements.extend(statements3);
                    AssertVariant::AssertNeq(left, right)
                }
            },
            ..input
        }
        .into();
        (stmt, statements)
    }

    fn reconstruct_assign(&mut self, input: AssignStatement) -> (Statement, Self::AdditionalOutput) {
        let AssignStatement { place, value, span, .. } = input;
        let mut statements = vec![];

        // Check if `place` is a path
        if let Expression::Path(path) = &place {
            // Check if the path corresponds to a global storage variable
            if let Some(global_location) = path.try_global_location() {
                let var = self
                    .state
                    .symbol_table
                    .lookup_global(self.program, global_location)
                    .expect("A global path must point to a global");

                // Storage variables that are not optional nor mappings are implicitly wrapped in an optional.
                assert!(
                    var.type_.as_ref().expect("must be known by now").is_optional(),
                    "Only storage variables that are not vectors or mappings are expected here."
                );

                // Reconstruct the RHS
                let (new_value, mut value_stmts) = self.reconstruct_expression(value, &());
                statements.append(&mut value_stmts);

                let id = || self.state.node_builder.next_id();
                let var_name = path.identifier().name;

                // Path to the mapping backing the storage variable: `<var_name>__`
                let mapping_symbol = Symbol::intern(&format!("{var_name}__"));
                let mapping_ident = Identifier::new(mapping_symbol, id());
                let mapping_expr: Expression =
                    Path::from(mapping_ident).to_global(Location::new(self.program, vec![mapping_symbol])).into();
                let false_literal: Expression = Literal::boolean(false, Span::default(), id()).into();

                let stmt = if matches!(new_value, Expression::Literal(Literal { variant: LiteralVariant::None, .. })) {
                    // Input:
                    //   storage x: field;
                    //   ...
                    //   x = none;
                    //
                    // Lowered reconstruction:
                    //   mapping x__: bool => field;
                    //   ...
                    //   _mapping_remove(x__, false);
                    let remove_expr: Expression = IntrinsicExpression {
                        name: sym::_mapping_remove,
                        type_parameters: vec![],
                        input_types: vec![],
                        return_types: vec![],
                        arguments: vec![mapping_expr, false_literal],
                        span,
                        id: id(),
                    }
                    .into();
                    Statement::Expression(ExpressionStatement { expression: remove_expr, span, id: id() })
                } else {
                    // Input:
                    //   storage x: field;
                    //   ...
                    //   x = 5field;
                    //
                    // Lowered reconstruction:
                    //   mapping x__: bool => field;
                    //   ...
                    //   _mapping_set(x__, false, 5field);
                    let set_expr: Expression = IntrinsicExpression {
                        name: sym::_mapping_set,
                        type_parameters: vec![],
                        input_types: vec![],
                        return_types: vec![],
                        arguments: vec![mapping_expr, false_literal, new_value],
                        span,
                        id: id(),
                    }
                    .into();
                    Statement::Expression(ExpressionStatement { expression: set_expr, span, id: id() })
                };
                return (stmt, statements);
            }
        }

        // In all other cases, nothing special to do.
        let (new_place, mut place_stmts) = self.reconstruct_expression(place, &());
        let (new_value, mut value_stmts) = self.reconstruct_expression(value, &());
        statements.append(&mut place_stmts);
        statements.append(&mut value_stmts);

        let stmt =
            AssignStatement { place: new_place, value: new_value, span, id: self.state.node_builder.next_id() }.into();
        (stmt, statements)
    }

    fn reconstruct_block(&mut self, block: Block) -> (Block, Self::AdditionalOutput) {
        let statements = self.reconstruct_block_statements(block.statements);

        (Block { span: block.span, statements, id: self.state.node_builder.next_id() }, Default::default())
    }

    fn reconstruct_conditional(&mut self, input: leo_ast::ConditionalStatement) -> (Statement, Self::AdditionalOutput) {
        let statements = self.reconstruct_conditional_with_tail(input, &[]);
        self.split_emitted_statement(statements)
    }

    fn reconstruct_const(&mut self, input: ConstDeclaration) -> (Statement, Self::AdditionalOutput) {
        let (type_expr, type_statements) = self.reconstruct_type(input.type_);
        let (value_expr, value_statements) = self.reconstruct_expression(input.value, &Default::default());

        let mut statements = Vec::new();
        statements.extend(type_statements);
        statements.extend(value_statements);

        (ConstDeclaration { type_: type_expr, value: value_expr, ..input }.into(), statements)
    }

    fn reconstruct_definition(&mut self, mut input: DefinitionStatement) -> (Statement, Self::AdditionalOutput) {
        let (new_value, additional_stmts) = self.reconstruct_expression(input.value, &());

        input.type_ = input.type_.map(|ty| self.reconstruct_type(ty).0);
        input.value = new_value;

        (input.into(), additional_stmts)
    }

    fn reconstruct_expression_statement(&mut self, input: ExpressionStatement) -> (Statement, Self::AdditionalOutput) {
        let keep_expression = !expression_can_be_discarded(&input.expression, self.state);
        let (reconstructed_expression, statements) = self.reconstruct_expression(input.expression, &Default::default());
        let legal_expression_statement = matches!(
            reconstructed_expression,
            Expression::Call(_) | Expression::DynamicOp(_) | Expression::Intrinsic(_)
        );
        if !legal_expression_statement && !keep_expression {
            (
                ExpressionStatement {
                    expression: Expression::Unit(UnitExpression {
                        span: Span::default(),
                        id: self.state.node_builder.next_id(),
                    }),
                    ..input
                }
                .into(),
                statements,
            )
        } else if !legal_expression_statement {
            // Type checking only permits call-like expression statements, so
            // preserve evaluation by binding the lowered expression to a local.
            let discard_sym = self.state.assigner.unique_symbol("$discard", "$");
            let discard_ident =
                Identifier { name: discard_sym, span: Default::default(), id: self.state.node_builder.next_id() };
            (
                DefinitionStatement {
                    place: DefinitionPlace::Single(discard_ident),
                    type_: None,
                    value: reconstructed_expression,
                    span: input.span,
                    id: self.state.node_builder.next_id(),
                }
                .into(),
                statements,
            )
        } else {
            (ExpressionStatement { expression: reconstructed_expression, ..input }.into(), statements)
        }
    }

    fn reconstruct_iteration(&mut self, _input: IterationStatement) -> (Statement, Self::AdditionalOutput) {
        panic!("`IterationStatement`s should not be in the AST at this point.");
    }

    fn reconstruct_return(&mut self, input: ReturnStatement) -> (Statement, Self::AdditionalOutput) {
        let (expression, statements) = self.reconstruct_expression(input.expression, &());
        (ReturnStatement { expression, ..input }.into(), statements)
    }
}
