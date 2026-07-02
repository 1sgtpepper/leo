/*
seed = 987654321
min_height = 16

[case]
program = "st_pop.aleo"
function = "main"
input = ["false"]
private_key = "APrivateKey1zkpH5Ne1Xfd79t61VhK7b6yaYz92yW5dbuVkiFheR7rwCDE"
*/

program st_pop.aleo {
    storage vec: [u32];

    fn main(take: bool) -> Final {
        return final {
            vec.push(99u32);
            let popped: u32? = take ? vec.pop() : none;
            assert(popped == none);
            assert(vec.len() == 1u32);
        };
    }

    @noupgrade
    constructor() {}
}
