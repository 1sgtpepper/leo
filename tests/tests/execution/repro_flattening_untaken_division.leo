/*
seed = 987654321

[case]
program = "flat_div.aleo"
function = "main"
input = ["false", "0u32"]
*/

program flat_div.aleo {
    fn main(take_bad: bool, denom: u32) -> u32 {
        if take_bad {
            let value: u32 = 1u32 / denom;
            return value;
        }
        return 0u32;
    }

    @noupgrade
    constructor() {}
}
