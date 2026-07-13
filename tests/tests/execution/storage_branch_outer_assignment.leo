/*
seed = 123456789
min_height = 16

[case]
program = "storage_branch_outer.aleo"
function = "run"
input = ["false"]

[case]
program = "storage_branch_outer.aleo"
function = "run"
input = ["true"]
*/

program storage_branch_outer.aleo {
    storage values: [u32];

    fn run(public take: bool) -> Final {
        return final {
            values.clear();
            values.push(9u32);
            let local: u32 = 0u32;
            let selected: u32? = take ? values.pop() : none;
            local = 1u32;
            assert(local == 1u32);
            assert(!take || selected != none);
            assert(take || selected == none);
            assert(values.len() == (take ? 0u32 : 1u32));
        };
    }

    @noupgrade
    constructor() {}
}
