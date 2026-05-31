/*
seed = 13579

[case]
program = "const_ternary_probe.aleo"
function = "literal_true_drops_div"
input = ["5u8"]

[case]
program = "const_ternary_probe.aleo"
function = "runtime_true_keeps_div"
input = ["true", "5u8"]
*/

program const_ternary_probe.aleo {
    fn literal_true_drops_div(x: u8) -> u8 {
        return true ? 1u8 : x / 0u8;
    }

    fn runtime_true_keeps_div(flag: bool, x: u8) -> u8 {
        return flag ? 1u8 : x / 0u8;
    }
}
