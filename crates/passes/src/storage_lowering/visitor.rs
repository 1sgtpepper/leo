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

use crate::{CompilerState, expression_can_be_discarded};

use leo_ast::*;
use leo_span::{Span, Symbol, sym};

use indexmap::IndexMap;

pub struct StorageLoweringVisitor<'a> {
    pub state: &'a mut CompilerState,
    // The name of the current program scope
    pub program: Symbol,
    pub new_mappings: IndexMap<Location, Mapping>,
}

impl StorageLoweringVisitor<'_> {
    pub(super) fn expression_type_for_materialization(&self, expression: &Expression) -> Option<Type> {
        if let Some(type_) = self.state.type_table.get(&expression.id()) {
            return Some(type_);
        }

        match expression {
            Expression::Binary(input) => {
                use BinaryOperation::*;
                match input.op {
                    And | Eq | Gte | Gt | Lte | Lt | Nand | Neq | Nor | Or => Some(Type::Boolean),
                    _ => self
                        .expression_type_for_materialization(&input.left)
                        .or_else(|| self.expression_type_for_materialization(&input.right)),
                }
            }
            Expression::Intrinsic(input) => match Intrinsic::from_symbol(input.name, &input.type_parameters) {
                Some(Intrinsic::MappingGet) => self.mapping_value_type(input.arguments.first()?),
                Some(Intrinsic::MappingGetOrUse) => self
                    .mapping_value_type(input.arguments.first()?)
                    .or_else(|| self.expression_type_for_materialization(input.arguments.get(2)?)),
                Some(Intrinsic::MappingContains) | Some(Intrinsic::DynamicContains) => Some(Type::Boolean),
                Some(Intrinsic::MappingSet) | Some(Intrinsic::MappingRemove) => Some(Type::Unit),
                Some(Intrinsic::DynamicGet) | Some(Intrinsic::DynamicGetOrUse) => {
                    input.type_parameters.first().map(|(type_, _)| type_.clone())
                }
                _ => None,
            },
            Expression::Literal(input) => match &input.variant {
                LiteralVariant::Address(_) => Some(Type::Address),
                LiteralVariant::Boolean(_) => Some(Type::Boolean),
                LiteralVariant::Field(_) => Some(Type::Field),
                LiteralVariant::Group(_) => Some(Type::Group),
                LiteralVariant::Identifier(_) => Some(Type::Identifier),
                LiteralVariant::Integer(integer_type, _) => Some(Type::Integer(*integer_type)),
                LiteralVariant::None => None,
                LiteralVariant::Scalar(_) => Some(Type::Scalar),
                LiteralVariant::Signature(_) => Some(Type::Signature),
                LiteralVariant::String(_) => Some(Type::String),
                LiteralVariant::Unsuffixed(_) => Some(Type::Integer(IntegerType::U32)),
            },
            Expression::Path(input) => self.path_type(input),
            Expression::Ternary(input) => match (
                self.expression_type_for_materialization(&input.if_true),
                self.expression_type_for_materialization(&input.if_false),
                input.if_true.is_none_expr(),
                input.if_false.is_none_expr(),
            ) {
                (Some(type_), _, _, true) | (_, Some(type_), true, _) => {
                    Some(Type::Optional(OptionalType { inner: Box::new(type_) }))
                }
                (Some(type_), _, _, _) => Some(type_),
                (_, Some(type_), _, _) => Some(type_),
                _ => None,
            },
            _ => None,
        }
    }

    fn path_type(&self, path: &Path) -> Option<Type> {
        self.state.type_table.get(&path.id()).or_else(|| {
            path.try_global_location().and_then(|location| {
                self.new_mappings
                    .get(location)
                    .map(|mapping| {
                        Type::Mapping(MappingType {
                            key: Box::new(mapping.key_type.clone()),
                            value: Box::new(mapping.value_type.clone()),
                        })
                    })
                    .or_else(|| {
                        self.state
                            .symbol_table
                            .lookup_global(self.program, location)
                            .and_then(|symbol| symbol.type_.clone())
                    })
            })
        })
    }

    fn mapping_value_type(&self, expression: &Expression) -> Option<Type> {
        match self.expression_type_for_materialization(expression)? {
            Type::Mapping(mapping) => Some(*mapping.value),
            _ => None,
        }
    }

    /// Returns the two mapping expressions that back a vector: `<base>__` (values)
    /// and `<base>__len__` (length).
    ///
    /// Panics if `expr` is not a `Path`.
    pub fn generate_vector_mapping_exprs(&mut self, path: &Path) -> (Expression, Expression) {
        let base = path.identifier().name;
        let val = Symbol::intern(&format!("{base}__"));
        let len = Symbol::intern(&format!("{base}__len__"));
        let element_type = match self.path_type(path) {
            Some(Type::Vector(VectorType { element_type })) => *element_type,
            _ => Type::Err,
        };

        let make_expr = |sym| {
            let ident = Identifier::new(sym, self.state.node_builder.next_id());
            let mut p = Path::from(ident);

            if let Some(program) = path.user_program().filter(|p| p.as_symbol() != self.program) {
                p = p.with_user_program(*program);
            }

            p.to_global(Location::new(self.program, vec![sym])).into()
        };

        let value_mapping: Expression = make_expr(val);
        self.state.type_table.insert(
            value_mapping.id(),
            Type::Mapping(MappingType {
                key: Box::new(Type::Integer(IntegerType::U32)),
                value: Box::new(element_type),
            }),
        );

        let len_mapping: Expression = make_expr(len);
        self.state.type_table.insert(
            len_mapping.id(),
            Type::Mapping(MappingType {
                key: Box::new(Type::Boolean),
                value: Box::new(Type::Integer(IntegerType::U32)),
            }),
        );

        (value_mapping, len_mapping)
    }

    pub fn literal_false(&mut self) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Boolean);
        Literal::boolean(false, Span::default(), id).into()
    }

    pub fn literal_none_optional(&mut self, inner_type: Type) -> Expression {
        let optional_type = Type::Optional(OptionalType { inner: Box::new(inner_type) });
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, optional_type);
        Literal::none(Span::default(), id).into()
    }

    pub fn literal_zero_u32(&mut self) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Integer(IntegerType::U32));
        Literal::integer(IntegerType::U32, "0".to_string(), Span::default(), id).into()
    }

    pub fn literal_one_u32(&mut self) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Integer(IntegerType::U32));
        Literal::integer(IntegerType::U32, "1".to_string(), Span::default(), id).into()
    }

    /// Generates `_mapping_get_or_use(len_path_expr, false, 0u32)`
    pub fn get_vector_len_expr(&mut self, len_path_expr: Expression, span: Span) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Integer(IntegerType::U32));
        IntrinsicExpression {
            name: sym::_mapping_get_or_use,
            type_parameters: vec![],
            input_types: vec![],
            return_types: vec![],
            arguments: vec![len_path_expr, self.literal_false(), self.literal_zero_u32()],
            span,
            id,
        }
        .into()
    }

    /// Generates `_mapping_set(path_expr, key_expr, value_expr)`
    pub fn set_mapping_expr(
        &mut self,
        path_expr: Expression,
        key_expr: Expression,
        value_expr: Expression,
        span: Span,
    ) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Unit);
        IntrinsicExpression {
            name: sym::_mapping_set,
            type_parameters: vec![],
            input_types: vec![],
            return_types: vec![],
            arguments: vec![path_expr, key_expr, value_expr],
            span,
            id,
        }
        .into()
    }

    /// Generates `_mapping_get(path_expr, key_expr)`
    pub fn get_mapping_expr(&mut self, path_expr: Expression, key_expr: Expression, span: Span) -> Expression {
        let id = self.state.node_builder.next_id();
        if let Some(type_) = self.mapping_value_type(&path_expr) {
            self.state.type_table.insert(id, type_);
        }
        IntrinsicExpression {
            name: sym::_mapping_get,
            type_parameters: vec![],
            input_types: vec![],
            return_types: vec![],
            arguments: vec![path_expr, key_expr],
            span,
            id,
        }
        .into()
    }

    /// Generates `_mapping_get_or_use(path_expr, key_expr, default_expr)`
    pub fn get_or_use_mapping_expr(
        &mut self,
        path_expr: Expression,
        key_expr: Expression,
        default_expr: Expression,
        span: Span,
    ) -> Expression {
        let id = self.state.node_builder.next_id();
        if let Some(type_) =
            self.mapping_value_type(&path_expr).or_else(|| self.expression_type_for_materialization(&default_expr))
        {
            self.state.type_table.insert(id, type_);
        }
        IntrinsicExpression {
            name: sym::_mapping_get_or_use,
            type_parameters: vec![],
            input_types: vec![],
            return_types: vec![],
            arguments: vec![path_expr, key_expr, default_expr],
            span,
            id,
        }
        .into()
    }

    pub fn ternary_expr(
        &mut self,
        condition: Expression,
        if_true: Expression,
        if_false: Expression,
        span: Span,
    ) -> Expression {
        let id = self.state.node_builder.next_id();
        let type_ = match (
            self.expression_type_for_materialization(&if_true),
            self.expression_type_for_materialization(&if_false),
            if_true.is_none_expr(),
            if_false.is_none_expr(),
        ) {
            (Some(type_), _, _, true) | (_, Some(type_), true, _) => {
                Some(Type::Optional(OptionalType { inner: Box::new(type_) }))
            }
            (Some(type_), _, _, _) => Some(type_),
            (_, Some(type_), _, _) => Some(type_),
            _ => None,
        };
        if let Some(type_) = type_ {
            self.state.type_table.insert(id, type_);
        }
        TernaryExpression { condition, if_true, if_false, span, id }.into()
    }

    /// Emits an identifier literal expression (e.g. `'x__'`).
    pub fn literal_identifier(&mut self, name: Symbol) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Identifier);
        Literal::identifier(name.to_string(), Span::default(), id).into()
    }

    /// Emits the default network literal `'aleo'`.
    pub fn literal_default_network(&mut self) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Identifier);
        Literal::identifier("aleo".to_string(), Span::default(), id).into()
    }

    /// Emits `_dynamic_contains(prog, net, mapping, key)`.
    pub fn dynamic_contains_expr(
        &mut self,
        prog: Expression,
        net: Expression,
        mapping: Expression,
        key: Expression,
        span: Span,
    ) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, Type::Boolean);
        IntrinsicExpression {
            name: sym::_dynamic_contains,
            type_parameters: vec![],
            input_types: vec![],
            return_types: vec![],
            arguments: vec![prog, net, mapping, key],
            span,
            id,
        }
        .into()
    }

    /// Emits `_dynamic_get_or_use::<value_ty>(prog, net, mapping, key, default)`.
    #[allow(clippy::too_many_arguments)]
    pub fn dynamic_get_or_use_expr(
        &mut self,
        prog: Expression,
        net: Expression,
        mapping: Expression,
        key: Expression,
        default: Expression,
        value_ty: Type,
        span: Span,
    ) -> Expression {
        let id = self.state.node_builder.next_id();
        self.state.type_table.insert(id, value_ty.clone());
        IntrinsicExpression {
            name: sym::_dynamic_get_or_use,
            type_parameters: vec![(value_ty, span)],
            input_types: vec![],
            return_types: vec![],
            arguments: vec![prog, net, mapping, key, default],
            span,
            id,
        }
        .into()
    }

    /// Looks up the interface referenced by an interface type expression.
    pub fn lookup_interface_from_type(&self, interface_ty: &Type) -> Interface {
        let Type::Composite(CompositeType { path, .. }) = interface_ty else {
            panic!("Dynamic access requires a composite interface type, got `{interface_ty}`");
        };
        let location = path.try_global_location().expect("interface path must resolve to a global location");
        self.state
            .symbol_table
            .lookup_interface(self.program, location)
            .expect("type checking guarantees the interface exists")
            .clone()
    }

    pub fn binary_expr(&mut self, left: Expression, op: BinaryOperation, right: Expression) -> Expression {
        let id = self.state.node_builder.next_id();
        let type_ = match op {
            BinaryOperation::And
            | BinaryOperation::Eq
            | BinaryOperation::Gte
            | BinaryOperation::Gt
            | BinaryOperation::Lte
            | BinaryOperation::Lt
            | BinaryOperation::Nand
            | BinaryOperation::Neq
            | BinaryOperation::Nor
            | BinaryOperation::Or => Some(Type::Boolean),
            _ => self
                .expression_type_for_materialization(&left)
                .or_else(|| self.expression_type_for_materialization(&right)),
        };
        if let Some(type_) = type_ {
            self.state.type_table.insert(id, type_);
        }
        BinaryExpression { op, left, right, span: Span::default(), id }.into()
    }

    /// Lowers `Interface@(target)::storage` (singleton bare read) to a ternary
    /// `contains.dynamic ? get_or_use.dynamic(..) : None` producing `Option<T>`.
    pub fn lower_dynamic_read(
        &mut self,
        interface_ty: Type,
        target_program: Expression,
        network: Option<Expression>,
        storage: Identifier,
        span: Span,
    ) -> (Expression, Vec<Statement>) {
        let interface = self.lookup_interface_from_type(&interface_ty);
        let storage_proto = interface
            .storages
            .iter()
            .find(|s| s.identifier.name == storage.name)
            .cloned()
            .expect("type checking guarantees storage exists in interface");

        let inner_type = match storage_proto.type_ {
            Type::Vector(_) => panic!("vector storage cannot be read as a singleton"),
            t => t,
        };

        let (prog_expr, prog_stmts) = self.reconstruct_expression(target_program, &());
        let (net_expr, net_stmts) = match network {
            Some(n) => self.reconstruct_expression(n, &()),
            None => (self.literal_default_network(), vec![]),
        };

        let mapping_sym = Symbol::intern(&format!("{}__", storage.name));
        let mapping_lit_a = self.literal_identifier(mapping_sym);
        let mapping_lit_b = self.literal_identifier(mapping_sym);
        let false_lit_a = self.literal_false();
        let false_lit_b = self.literal_false();

        let contains_expr =
            self.dynamic_contains_expr(prog_expr.clone(), net_expr.clone(), mapping_lit_a, false_lit_a, span);

        let zero = self.zero(&inner_type);
        let get_or_use_expr = self.dynamic_get_or_use_expr(
            prog_expr,
            net_expr,
            mapping_lit_b,
            false_lit_b,
            zero,
            inner_type.clone(),
            span,
        );
        let none_expr = self.literal_none_optional(inner_type);
        let ternary = self.ternary_expr(contains_expr, get_or_use_expr, none_expr, span);

        let mut stmts = prog_stmts;
        stmts.extend(net_stmts);
        (ternary, stmts)
    }

    /// Lowers `Interface@(target)::vec.get(i)` to a ternary checking `i < len` and
    /// reading `<base>__[i]` from the backing mapping, producing `Option<element>`.
    pub fn lower_dynamic_vector_get(
        &mut self,
        target_program: Expression,
        network: Option<Expression>,
        member: Identifier,
        element_type: Type,
        arguments: Vec<Expression>,
        span: Span,
    ) -> (Expression, Vec<Statement>) {
        let (prog_expr, prog_stmts) = self.reconstruct_expression(target_program, &());
        let (net_expr, net_stmts) = match network {
            Some(n) => self.reconstruct_expression(n, &()),
            None => (self.literal_default_network(), vec![]),
        };
        let index_argument = arguments.into_iter().next().expect("type checking guarantees one argument");
        let index_type = self
            .state
            .type_table
            .get(&index_argument.id())
            .expect("type checking should assign a type to the vector index");
        let index_must_be_evaluated_once = !expression_can_be_discarded(&index_argument, self.state);
        let (index_expr, mut index_stmts) = self.reconstruct_expression(index_argument, &());
        self.state.type_table.insert(index_expr.id(), index_type.clone());
        let index_expr = if index_must_be_evaluated_once {
            let index_var_sym = self.state.assigner.unique_symbol("$index", "$");
            let index_var_ident =
                Identifier { name: index_var_sym, span: Default::default(), id: self.state.node_builder.next_id() };
            self.state.type_table.insert(index_var_ident.id, index_type);
            index_stmts.push(self.state.assigner.simple_definition(
                index_var_ident,
                index_expr,
                self.state.node_builder.next_id(),
            ));
            Path::from(index_var_ident).to_local().into()
        } else {
            index_expr
        };

        let base_name = member.name.to_string();
        let val_mapping_sym = Symbol::intern(&format!("{base_name}__"));
        let len_mapping_sym = Symbol::intern(&format!("{base_name}__len__"));

        let len_mapping_lit = self.literal_identifier(len_mapping_sym);
        let false_lit = self.literal_false();
        let zero_u32 = self.literal_zero_u32();
        let get_len_expr = self.dynamic_get_or_use_expr(
            prog_expr.clone(),
            net_expr.clone(),
            len_mapping_lit,
            false_lit,
            zero_u32,
            Type::Integer(IntegerType::U32),
            span,
        );
        let len_var_sym = self.state.assigner.unique_symbol("$len_var", "$");
        let len_var_ident =
            Identifier { name: len_var_sym, span: Default::default(), id: self.state.node_builder.next_id() };
        let len_stmt =
            self.state.assigner.simple_definition(len_var_ident, get_len_expr, self.state.node_builder.next_id());
        let len_var_expr: Expression = Path::from(len_var_ident).to_local().into();

        let index_lt_len_expr = self.binary_expr(index_expr.clone(), BinaryOperation::Lt, len_var_expr);

        let val_mapping_lit = self.literal_identifier(val_mapping_sym);
        let zero = self.zero(&element_type);
        let get_or_use_expr = self.dynamic_get_or_use_expr(
            prog_expr,
            net_expr,
            val_mapping_lit,
            index_expr,
            zero,
            element_type.clone(),
            span,
        );
        let none_expr = self.literal_none_optional(element_type);
        let ternary = self.ternary_expr(index_lt_len_expr, get_or_use_expr, none_expr, span);

        let mut stmts = prog_stmts;
        stmts.extend(net_stmts);
        stmts.extend(index_stmts);
        stmts.push(len_stmt);

        (ternary, stmts)
    }

    /// Lowers `Interface@(target)::vec.len()` to a `_dynamic_get_or_use::<u32>` read of the
    /// backing `<base>__len__` mapping, defaulting to `0u32` when the length has not been set.
    pub fn lower_dynamic_vector_len(
        &mut self,
        target_program: Expression,
        network: Option<Expression>,
        member: Identifier,
        span: Span,
    ) -> (Expression, Vec<Statement>) {
        let (prog_expr, prog_stmts) = self.reconstruct_expression(target_program, &());
        let (net_expr, net_stmts) = match network {
            Some(n) => self.reconstruct_expression(n, &()),
            None => (self.literal_default_network(), vec![]),
        };

        let len_mapping_sym = Symbol::intern(&format!("{}__len__", member.name));
        let len_mapping_lit = self.literal_identifier(len_mapping_sym);
        let false_lit = self.literal_false();
        let zero_u32 = self.literal_zero_u32();
        let expr = self.dynamic_get_or_use_expr(
            prog_expr,
            net_expr,
            len_mapping_lit,
            false_lit,
            zero_u32,
            Type::Integer(IntegerType::U32),
            span,
        );

        let mut stmts = prog_stmts;
        stmts.extend(net_stmts);
        (expr, stmts)
    }

    /// Produces a zero expression for `Type` `ty`.
    pub fn zero(&self, ty: &Type) -> Expression {
        // zero value for element type (used as default in get_or_use)
        let symbol_table = &self.state.symbol_table;
        let struct_lookup = |loc: &Location| {
            symbol_table
                .lookup_struct(self.program, loc)
                .unwrap()
                .members
                .iter()
                .map(|mem| (mem.identifier.name, mem.type_.clone()))
                .collect()
        };
        Expression::zero(ty, Span::default(), &self.state.node_builder, &struct_lookup)
            .expect("zero value generation failed")
    }

    pub fn reconstruct_path_or_locator(&mut self, input: Expression) -> (Expression, Vec<Statement>) {
        let location = match input {
            Expression::Path(ref path) if path.is_local() => {
                // nothing to do for local paths.
                return (input, vec![]);
            }
            Expression::Path(ref path) => {
                // Otherwise, it should be a global path.
                path.expect_global_location().clone()
            }
            _ => panic!("unexpected expression type"),
        };

        // Check if this path corresponds to a global symbol.
        let Some(var) = self.state.symbol_table.lookup_global(self.program, &location) else {
            // Nothing to do
            return (input, vec![]);
        };
        let var_type = var.type_.clone();

        match var_type {
            Some(Type::Mapping(_)) => {
                // No transformation needed for mappings.
                (input, vec![])
            }

            Some(Type::Optional(OptionalType { inner })) => {
                // Input:
                //   storage x: field;
                //   ...
                //   let y = x;
                //
                // Lowered reconstruction:
                //  mapping x__: bool => field
                //  let y = x__.contains(false)
                //      ? x__.get_or_use(false, 0field)
                //      : None;

                let var_name = location.path.last().unwrap();

                // Path to the mapping backing the optional variable: `<var_name>__`
                let mapping_symbol = Symbol::intern(&format!("{var_name}__"));
                let mapping_ident = Identifier::new(mapping_symbol, self.state.node_builder.next_id());

                // === Build expressions ===
                let mapping_expr: Expression = {
                    let path = if let Expression::Path(path) = input {
                        path
                    } else {
                        panic!("unexpected expression type");
                    };

                    let mut base_path = Path::from(mapping_ident);

                    // Attach user program only if it's present and different from current
                    if let Some(user_program) = path.user_program()
                        && user_program.as_symbol() != self.program
                    {
                        base_path = base_path.with_user_program(*user_program);
                    }

                    base_path.to_global(Location::new(self.program, vec![mapping_ident.name])).into()
                };
                self.state.type_table.insert(
                    mapping_expr.id(),
                    Type::Mapping(MappingType { key: Box::new(Type::Boolean), value: inner.clone() }),
                );

                let false_literal = self.literal_false();

                // `<var_name>__.contains(false)`
                let contains_id = self.state.node_builder.next_id();
                self.state.type_table.insert(contains_id, Type::Boolean);
                let contains_expr: Expression = IntrinsicExpression {
                    name: sym::_mapping_contains,
                    type_parameters: vec![],
                    input_types: vec![],
                    return_types: vec![],
                    arguments: vec![mapping_expr.clone(), false_literal.clone()],
                    span: Span::default(),
                    id: contains_id,
                }
                .into();

                // zero value for element type
                let zero = self.zero(&inner);

                // `<var_name>__.get_or_use(false, zero_value)`
                let get_or_use_expr =
                    self.get_or_use_mapping_expr(mapping_expr.clone(), false_literal, zero, Span::default());
                let none_expr = self.literal_none_optional(*inner.clone());

                (self.ternary_expr(contains_expr, get_or_use_expr, none_expr, Span::default()), vec![])
            }

            _ => {
                panic!("Expected an optional or a mapping, found {:?}", var_type);
            }
        }
    }
}
