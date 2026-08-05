program second.aleo {
    fn main(public value: u32) -> u32 {
        return value + 1u32;
    }

    @noupgrade
    constructor() {}
}
