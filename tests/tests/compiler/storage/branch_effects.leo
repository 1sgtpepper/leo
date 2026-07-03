program test.aleo {
    @noupgrade
    constructor() {}

    mapping branch_map: u32 => u32;
    storage optional_slot: u32;
    storage len_vec: [u32];

    fn mapping_get_unselected(public take: bool) -> Final {
        return final {
            let value: u32 = take ? branch_map.get(0u32) : 0u32;
            assert(value == 0u32);
        };
    }

    fn mapping_get_or_use_unselected(public take: bool) -> Final {
        return final {
            let value: u32 = take ? branch_map.get_or_use(0u32, 1u32) : 0u32;
            assert(value == 0u32);
        };
    }

    fn mapping_contains_unselected(public take: bool) -> Final {
        return final {
            let value: bool = take ? branch_map.contains(0u32) : false;
            assert(!value);
        };
    }

    fn optional_path_unselected(public take: bool) -> Final {
        return final {
            let value: u32? = take ? optional_slot : none;
            assert(value == none);
        };
    }

    fn vector_len_unselected(public take: bool) -> Final {
        return final {
            len_vec.push(11u32);
            let len: u32 = take ? len_vec.len() : 0u32;
            assert(len == 0u32);
            assert(len_vec.len() == 1u32);
        };
    }
}
