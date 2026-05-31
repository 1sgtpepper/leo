/*
Test that an untaken conditional assignment branch is not evaluated.

[case]
program = "conditional_branch_probe.aleo"
function = "main"
input = ["true", "7u8"]

[case]
program = "conditional_branch_probe.aleo"
function = "main"
input = ["false", "7u8"]
*/

program conditional_branch_probe.aleo {
    fn main(flag: bool, x: u8) -> u8 {
        let y: u8 = 0u8;
        if flag {
            y = 1u8;
        } else {
            y = x / 0u8;
        }
        return y;
    }
}
