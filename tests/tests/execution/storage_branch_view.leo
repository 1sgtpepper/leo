/*
[case]
program = "branch_view.aleo"
function = "read_or_divide"
input = ["true", "0u32"]
seed_mapping = ["values__len__", "false", "3u32"]

[case]
program = "branch_view.aleo"
function = "read_or_divide"
input = ["true", "2u32"]
seed_mapping = ["values__len__", "false", "3u32"]

[case]
program = "branch_view.aleo"
function = "read_or_divide"
input = ["false", "2u32"]
seed_mapping = ["values__len__", "false", "3u32"]

[case]
program = "branch_view.aleo"
function = "read_or_divide"
input = ["false", "0u32"]
seed_mapping = ["values__len__", "false", "3u32"]
*/

program branch_view.aleo {
    storage values: [u32];

    fn dummy(value: u32) -> u32 {
        return value;
    }

    view fn read_or_divide(take_storage: bool, denominator: u32) -> u32 {
        return take_storage ? values.len() : 10u32 / denominator;
    }

    @noupgrade
    constructor() {}
}
