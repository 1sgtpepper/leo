/*
Unused invalid field deserialization should still fail at execution.

[case]
program = "deserialize_unused_probe.aleo"
function = "main"
input = []
*/

program deserialize_unused_probe.aleo {
    fn main() -> u8 {
        let bits: [bool; 253] = [true; 253];
        let value: field = Deserialize::from_bits_raw::[field](bits);
        return 1u8;
    }
}
