/*
seed = 123456789
min_height = 16

[case]
program = "branch_join_assignment.aleo"
function = "run"
input = ["false"]

[case]
program = "branch_join_assignment.aleo"
function = "run"
input = ["true"]
*/

program branch_join_assignment.aleo {
    storage values: [u32];
    mapping audit: bool => u32;

    fn run(public take: bool) -> Final {
        return final {
            values.clear();
            values.push(9u32);
            let retained: u32 = take ? 3u32 : values.get(0u32).unwrap_or(0u32);
            assert(retained == (take ? 3u32 : 9u32));
            let local: u32 = 0u32;
            let selected: u32 = take ? values.pop().unwrap_or(0u32) : 4u32;
            local = selected + 1u32;
            Mapping::set(audit, false, local);
            assert(local == (take ? 10u32 : 5u32));
            assert(values.len() == (take ? 0u32 : 1u32));
        };
    }

    @noupgrade
    constructor() {}
}
