/*
seed = 123456789
min_height = 16

[case]
program = "storage_branch_local_versions.aleo"
function = "run"
input = ["false"]

[case]
program = "storage_branch_local_versions.aleo"
function = "run"
input = ["true"]
*/

export struct Pair {
    x: u32,
    y: u32,
}

export struct Nested {
    pair: Pair,
    values: [u32; 2],
}

program storage_branch_local_versions.aleo {
    storage values: [u32];

    fn run(public take: bool) -> Final {
        return final {
            values.clear();
            values.push(9u32);
            let local: u32 = 0u32;
            let aggregate: Pair = Pair { x: 1u32, y: 2u32 };
            let tuple: (u32, u32) = (3u32, 4u32);
            let array: [u32; 2] = [5u32, 6u32];
            let nested: Nested = Nested {
                pair: Pair { x: 7u32, y: 8u32 },
                values: [9u32, 10u32],
            };

            if take {
                let selected: u32? = values.pop();
                let local_version: u32 = local;
                let aggregate_version: Pair = aggregate;
                let tuple_version: (u32, u32) = tuple;
                let array_version: [u32; 2] = array;
                let nested_version: Nested = nested;
                local_version += 1u32;
                aggregate_version.x += 2u32;
                tuple_version.0 += 3u32;
                array_version[1u32] += 4u32;
                nested_version.pair.y += 5u32;
                nested_version.values[0u32] += 6u32;
                let (tuple_left, tuple_right): (u32, u32) = tuple_version;
                assert(selected != none);
                assert(local_version == 1u32);
                assert(aggregate_version.x == 3u32);
                assert(tuple_left == 6u32);
                assert(tuple_right == 4u32);
                assert(array_version[1u32] == 10u32);
                assert(nested_version.pair.y == 13u32);
                assert(nested_version.values[0u32] == 15u32);
            } else {
                let selected: u32? = none;
                let local_version: u32 = local;
                let aggregate_version: Pair = aggregate;
                let tuple_version: (u32, u32) = tuple;
                let array_version: [u32; 2] = array;
                let nested_version: Nested = nested;
                local_version += 1u32;
                aggregate_version.x += 2u32;
                tuple_version.0 += 3u32;
                array_version[1u32] += 4u32;
                nested_version.pair.y += 5u32;
                nested_version.values[0u32] += 6u32;
                let (tuple_left, tuple_right): (u32, u32) = tuple_version;
                assert(selected == none);
                assert(local_version == 1u32);
                assert(aggregate_version.x == 3u32);
                assert(tuple_left == 6u32);
                assert(tuple_right == 4u32);
                assert(array_version[1u32] == 10u32);
                assert(nested_version.pair.y == 13u32);
                assert(nested_version.values[0u32] == 15u32);
            }

            // This suffix uses no branch-local value and is therefore legal after the rejoin.
            assert(values.len() == (take ? 0u32 : 1u32));
        };
    }

    @noupgrade
    constructor() {}
}
