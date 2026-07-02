/*
seed = 987654321

[case]
program = "cast_eq.aleo"
function = "main"
input = ["-1i8"]
*/

program cast_eq.aleo {
    fn main(x: i8) -> i8 {
        let y: i8 = (x as u8) as i8;
        return y;
    }

    @noupgrade
    constructor() {}
}
