/*
Used invalid field deserialization should fail at execution.

[case]
program = "deserialize_used_probe.aleo"
function = "main"
input = []
*/

program deserialize_used_probe.aleo {
    fn main() -> field {
        let bits: [bool; 253] = [true; 253];
        let value: field = Deserialize::from_bits_raw::[field](bits);
        return value;
    }
}
