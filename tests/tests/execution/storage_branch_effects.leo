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

*/

struct Update {
    x: u32,
    y: u32,
}

program storage_branch_effects.aleo {

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

}
