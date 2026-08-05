import main.aleo;

program test_main.aleo {
    @test
    fn test_main() {
        let result: u32 = main.aleo::main(1u32);
        assert_eq(result, 1u32);
    }

    @noupgrade
    constructor() {}
}
