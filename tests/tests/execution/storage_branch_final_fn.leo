/*
seed = 123456789
min_height = 16

[case]
program = "branch_final_fn.aleo"
function = "run"
input = ["false"]

[case]
program = "branch_final_fn.aleo"
function = "run"
input = ["true"]
*/

program branch_final_fn.aleo {
    storage values: [u32];
    mapping audit: bool => u32;

    fn run(public take: bool) -> Final {
        return final {
            values.clear();
            values.push(9u32);
            let selected: u32? = select_and_pop(take);
            let nine: u32? = 9u32;
            assert(!take || selected == nine);
            assert(take || selected == none);
            Mapping::set(audit, false, values.len());
            assert(values.len() == (take ? 0u32 : 1u32));
        };
    }

    @noupgrade
    constructor() {}
}

export final fn select_and_pop(take: bool) -> u32? {
    return take ? values.pop() : none;
}
