/*
seed = 123456789
min_height = 16

[case]
program = "storage_branch_effects.aleo"
function = "definition_false"
input = ["false"]

[case]
program = "storage_branch_effects.aleo"
function = "definition_true"
input = ["true"]

[case]
program = "storage_branch_effects.aleo"
function = "assert_owner_false"
input = ["false"]

[case]
program = "storage_branch_effects.aleo"
function = "assign_owner_false"
input = ["false"]

[case]
program = "storage_branch_effects.aleo"
function = "condition_owner_false"
input = ["false"]

[case]
program = "storage_branch_effects.aleo"
function = "nested_ternary_false_inner"
input = ["true", "false"]

[case]
program = "storage_branch_effects.aleo"
function = "else_if_guarded"
input = ["false"]

[case]
program = "storage_branch_effects.aleo"
function = "else_if_source_ternary"
input = ["false", "false"]

[case]
program = "storage_branch_effects.aleo"
function = "prefix_order_tuple"
input = []

[case]
program = "storage_branch_effects.aleo"
function = "prefix_order_reconstructed_read"
input = ["false", "1u32"]

[case]
program = "storage_branch_effects.aleo"
function = "prefix_order_reconstructed_read"
input = ["false", "0u32"]

[case]
program = "storage_branch_effects.aleo"
function = "composite_base_before_storage"
input = ["1u32"]

[case]
program = "storage_branch_effects.aleo"
function = "composite_base_before_storage"
input = ["0u32"]

[case]
program = "storage_branch_effects.aleo"
function = "dependent_ternary_chain"
input = ["false", "false", "false", "false", "false", "false", "false", "false", "false", "false", "false", "false", "false", "false", "false"]
*/

program storage_branch_effects.aleo {
    struct Update {
        x: u32,
        y: u32,
    }

    @noupgrade
    constructor() {}

    storage def_false_vec: [u32];
    storage def_true_vec: [u32];
    storage assert_vec: [u32];
    storage assign_vec: [u32];
    storage assign_value: u32;
    storage cond_vec: [u32];
    storage nested_vec: [u32];
    storage else_if_vec: [u32];
    storage else_if_ternary_vec: [u32];
    storage prefix_vec: [u32];
    storage prefix_optional: u32?;
    storage chain_vec: [u32];

    fn definition_false(public take: bool) -> Final {
        return final {
            def_false_vec.push(1u32);
            let popped: u32? = take ? def_false_vec.pop() : none;
            assert(popped == none);
            assert(def_false_vec.len() == 1u32);
        };
    }

    fn definition_true(public take: bool) -> Final {
        return final {
            def_true_vec.push(2u32);
            let popped: u32? = take ? def_true_vec.pop() : none;
            assert(popped != none);
            assert(popped.unwrap() == 2u32);
            assert(def_true_vec.len() == 0u32);
        };
    }

    fn assert_owner_false(public take: bool) -> Final {
        return final {
            assert_vec.push(3u32);
            assert((take ? assert_vec.pop() : assert_vec.get(999u32)) == none);
            assert(assert_vec.len() == 1u32);
        };
    }

    fn assign_owner_false(public take: bool) -> Final {
        return final {
            assign_vec.push(4u32);
            assign_value = take ? assign_vec.pop().unwrap() : 0u32;
            assert(assign_value.unwrap() == 0u32);
            assert(assign_vec.len() == 1u32);
        };
    }

    fn condition_owner_false(public take: bool) -> Final {
        return final {
            cond_vec.push(5u32);
            if (take ? cond_vec.pop() : cond_vec.get(999u32)) != none {
                assert(false);
            }
            assert(cond_vec.len() == 1u32);
        };
    }

    fn nested_ternary_false_inner(public outer: bool, public inner: bool) -> Final {
        return final {
            nested_vec.push(6u32);
            let popped: u32? = outer ? (inner ? nested_vec.pop() : none) : none;
            assert(popped == none);
            assert(nested_vec.len() == 1u32);
        };
    }

    fn else_if_guarded(public take: bool) -> Final {
        return final {
            else_if_vec.push(7u32);
            if !take {
                assert(else_if_vec.len() == 1u32);
            } else if else_if_vec.pop() != none {
                assert(false);
            }
            assert(else_if_vec.len() == 1u32);
        };
    }

    fn else_if_source_ternary(public first: bool, public take: bool) -> Final {
        return final {
            else_if_ternary_vec.push(8u32);
            if first {
                assert(false);
            } else if (take ? else_if_ternary_vec.pop() : else_if_ternary_vec.get(999u32)) != none {
                assert(false);
            }
            assert(else_if_ternary_vec.len() == 1u32);
        };
    }

    fn prefix_order_tuple() -> Final {
        return final {
            prefix_vec.push(9u32);
            prefix_vec.push(10u32);
            let pair: (u32?, u32?) = (prefix_vec.pop(), prefix_vec.len() == 1u32 ? none : prefix_vec.pop());
            assert(pair.0 != none);
            assert(pair.0.unwrap() == 10u32);
            assert(pair.1 == none);
            assert(prefix_vec.len() == 1u32);
        };
    }

    fn prefix_order_reconstructed_read(public take: bool, public denominator: u32) -> Final {
        return final {
            let pair: (u32, u32?) = (1u32 / denominator, take ? prefix_optional : none);
            assert(pair.0 == 1u32);
            assert(pair.1 == none);
        };
    }

    fn composite_base_before_storage(public denominator: u32) -> Final {
        return final {
            prefix_vec.push(11u32);
            let pair: (Update, u32?) = (
                Update { x: 7u32, ..Update { x: 1u32 / denominator, y: 0u32 } },
                prefix_vec.pop(),
            );
            assert(pair.0.x == 7u32);
            assert(pair.1.unwrap() == 11u32);
        };
    }

    fn dependent_ternary_chain(
        public c1: bool,
        public c2: bool,
        public c3: bool,
        public c4: bool,
        public c5: bool,
        public c6: bool,
        public c7: bool,
        public c8: bool,
        public c9: bool,
        public c10: bool,
        public c11: bool,
        public c12: bool,
        public c13: bool,
        public c14: bool,
        public c15: bool,
    ) -> Final {
        return final {
            let x1: u32? = c1 ? chain_vec.pop() : none;
            let x2: u32? = c2 ? chain_vec.pop() : x1;
            let x3: u32? = c3 ? chain_vec.pop() : x2;
            let x4: u32? = c4 ? chain_vec.pop() : x3;
            let x5: u32? = c5 ? chain_vec.pop() : x4;
            let x6: u32? = c6 ? chain_vec.pop() : x5;
            let x7: u32? = c7 ? chain_vec.pop() : x6;
            let x8: u32? = c8 ? chain_vec.pop() : x7;
            let x9: u32? = c9 ? chain_vec.pop() : x8;
            let x10: u32? = c10 ? chain_vec.pop() : x9;
            let x11: u32? = c11 ? chain_vec.pop() : x10;
            let x12: u32? = c12 ? chain_vec.pop() : x11;
            let x13: u32? = c13 ? chain_vec.pop() : x12;
            let x14: u32? = c14 ? chain_vec.pop() : x13;
            let x15: u32? = c15 ? chain_vec.pop() : x14;
            assert(x15 == none);
        };
    }
}
