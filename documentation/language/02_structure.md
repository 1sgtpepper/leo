---
id: structure
title: Structure of a Leo Program
sidebar_label: Program Structure
---

[general tags]: # "program, constant, import, record, struct, mapping, identifiers, naming, underscore"

## Layout of a Leo Program

A Leo program contains declarations of a [Program](#program), [Constants](#constant), [Imports](#import)
, [Structs](#struct), [Records](#record), [Mappings](#mappings), [Interfaces](./programs_in_practice/interfaces.md), and functions.
Declarations are locally accessible within a program file.
If you need a declaration from another Leo file, you must import it.

## Identifiers

All Leo identifiers — program names, function names, variables, struct and
record names, fields, mappings, storage variables, constants, and interface
names — share a common shape, with extra restrictions for identifiers whose
name reaches the compiled Aleo bytecode.

### General rules

A Leo identifier:

- Begins with an ASCII letter (`a`–`z`, `A`–`Z`) or, in the positions listed
  below, a single underscore (`_`).
- Continues with ASCII letters, digits (`0`–`9`), and single underscores.
- Cannot contain a double underscore (`__`).
- Cannot be exactly equal to a Leo keyword (`let`, `const`, `fn`, `record`,
  `mapping`, `program`, …), a SnarkVM reserved keyword, or the literal `aleo`.
- Cannot match a bare-callable intrinsic name (`_self_caller`, `_block_height`,
  …). The parser dispatches an intrinsic call before any scope lookup, so a
  same-named local would be silently shadowed and is rejected at compile time.

Two additional restrictions apply only to identifiers whose name is emitted
into the Aleo bytecode:

- **Program names, record names, and record member names** cannot contain the
  substring `aleo` anywhere (e.g. `my_aleo_token` is rejected). This rule does
  not apply to struct names or to identifiers below the bytecode boundary
  (locals, parameters, helper functions, mappings, storage variables).
- A leading underscore is rejected in these positions; see
  [Leading underscore](#leading-underscore-_) below.

For the conventional casing of each kind of identifier (CamelCase vs.
snake_case), see [Naming Conventions](./07_style.md#naming-conventions). This
section describes only what the compiler accepts.

### Leading underscore (`_`)

Leo follows `rustc`'s `_x` convention for marking a binding "intentionally
unused" and silencing the matching
[`UNU` warning](./09_diagnostics.md#the-unu-family--unused-items). Whether a
leading `_` is permitted depends on whether the name is emitted into the
compiled Aleo bytecode. snarkVM identifiers must start with a letter, so any
Leo identifier that reaches the bytecode must also start with a letter — Leo
rejects the bad cases at compile time rather than letting them surface as a
deploy-time failure.

**Allowed positions** (silences the corresponding `unused_*` warning):

| Position                                                                | Silences           |
| ----------------------------------------------------------------------- | ------------------ |
| Local `let` binding                                                     | `unused_variable`  |
| Tuple-pattern element                                                   | `unused_variable`  |
| Local `const`                                                           | `unused_const`     |
| Top-level / module-scope `const`                                        | `unused_const`     |
| Loop iteration variable                                                 | `unused_variable`  |
| Function parameter on a non-entry, non-`@test` function                 | `unused_variable`  |
| Const-generic parameter                                                 | `unused_variable`  |
| Free `fn` name (force-inlined; the name never reaches the VM)           | `unused_function`  |
| `final fn` name (always inlined)                                        | `unused_function`  |
| Interface name (Leo-only, no bytecode emission)                         | n/a                |

**Rejected positions** — every identifier whose name is emitted verbatim into
the Aleo bytecode:

- Program names (e.g. `program _foo.aleo`)
- Entry-point function names (`fn` inside `program { … }`)
- Struct and record names
- Struct and record field names
- Mapping names
- Storage variable names

**Rejected for other reasons:**

- A binding whose name matches a bare-callable intrinsic (`_self_caller`,
  `_block_height`, etc.) is rejected by the parser.
- A free `fn` annotated with `@no_inline` and given a `_`-prefixed name is
  rejected by the type checker: `@no_inline` keeps the name in the bytecode,
  which contradicts the `_`-prefix silencing marker.

**Reading a `_`-prefixed binding** defeats the silencing intent and emits the
`used underscore binding` warning. Either remove the leading `_` or stop
reading the binding.

### Program name (program ID)

A program ID is declared as `{name}.{network}` and follows the general rules
above with two additional restrictions:

- The `name` may only contain lowercase letters, digits, and single
  underscores — uppercase letters are not permitted.
- The `name` cannot contain the substring `aleo` anywhere.

Currently, `aleo` is the only supported `network` domain.

```leo showLineNumbers
program hello.aleo;     // valid

program Foo.aleo;       // invalid — uppercase letter
program baR.aleo;       // invalid — uppercase letter
program 0foo.aleo;      // invalid — leading digit
program 0_foo.aleo;     // invalid — leading digit
program _foo.aleo;      // invalid — leading underscore
program foo__bar.aleo;  // invalid — double underscore
program aleo.aleo;      // invalid — contains `aleo`
program my_aleo.aleo;   // invalid — contains `aleo`
```

### Program

A program is a collection of code (its functions) and data (its types) that resides at a
[program ID](#program-name-program-id) on the Aleo blockchain. A program is declared as `program {name}.{network} { ... }`.
The body of the program is delimited by curly braces `{}`.

```leo file=../code_snippets/layout/main_example/src/main.leo#file
```

The following must be declared inside the scope of a program in a Leo file:

- mappings
- storage variables
- record types
- entry point `fn` declarations

The following must be declared outside the scope of a program in a Leo file:

- imports
- struct types
- helper `fn` definitions
- `final fn` definitions
- `interface` definitions

For the naming rules that apply to the program ID, see
[Program name (program ID)](#program-name-program-id) above.

### Constant

A constant is declared as `const {name}: {type} = {expression};`.
Constants are immutable, and the right-hand side must be an expression evaluatable at compile time.

Constants can be declared in four scopes:

- **Global scope** (outside all program blocks in a program file): accessible anywhere in the same file.
- **Program scope** (inside a `program` block, outside any function): accessible within that program.
- **Local scope** (inside a function body): accessible only within that function.
- **Module scope** (in a module file within the same package, i.e. a `.leo` file with no `program` block): accessible within the same package via `path::to::module::CONST_NAME`. See [Modules](./01_layout.md#modules) for details.

Constants are also supported in [libraries](./06_libraries.md), which are separate packages containing reusable code. A library's root file and its submodules may declare constants, accessible from any dependent package as `library::CONST_NAME` or `library::path::to::submodule::CONST_NAME`.

**Accessibility across packages:** Global and program-scope constants in a program are accessible from other programs that import it, using `program_name.aleo::CONST_NAME`. The following are not yet supported:

1. Accessing a constant from a submodule of an imported program
2. Accessing an imported program's constant from within a submodule of the current program

See [ProvableHQ/leo#29274](https://github.com/ProvableHQ/leo/issues/29274).

```leo file=../code_snippets/structure/constants/src/main.leo#scopes
```

**Supported types:** All integer types (`u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64`, `i128`), `bool`, `field`, `group`, `scalar`, `address`, and tuples, arrays, and structs composed of these types.

**Compile-time expressions:** The right-hand side of a constant declaration must be evaluatable at compile time. Valid right-hand sides include:

- Literal values (e.g., `42u32`, `true`, `1field`)
- References to previously declared constants
- Arithmetic, bitwise, and comparison expressions over constants (e.g., `MAX * 2u64`, `!FLAG`)
- Tuple, array, and struct expressions whose components are themselves compile-time constants

```leo file=../code_snippets/structure/constants/src/main.leo#expressions
```

### Import

You can import dependencies that are downloaded to the `imports` directory.
An import is declared as `import {filename}.aleo;`
The dependency resolver will pull the imported program from the network or the local filesystem.

```leo file=../code_snippets/layout/import_only/src/main.leo#snippet showLineNumbers
```

### Mappings

A mapping is declared as `mapping {name}: {key-type} => {value-type}`.
Mappings contain key-value pairs and are stored on chain.

```leo file=../code_snippets/structure/declarations/src/main.leo#mapping
```

### Storage

A storage variable is declared as `storage {name}: {type}`. Storage variables contain singleton values. They are declared at program scope and are stored on chain, similar to mappings.

```leo file=../code_snippets/structure/declarations/src/main.leo#storage_var
```

A storage vector is declared as `storage {name}: [{type}]`. Storage vectors contain dynamic lists of values of a given type. They are declared at program scope and are stored on chain, similar to mappings.

```leo file=../code_snippets/structure/declarations/src/main.leo#storage_vec
```

### Struct

A struct data type is declared as `struct {name} {}`.
Structs contain component declarations `{name}: {type},`.

```leo file=../code_snippets/structure/declarations/src/main.leo#struct showLineNumbers
```

### Record

A [record](https://docs.aleo.org/learn/core-concepts/public-and-private-state#private-state) data type is declared as `record {name} {}`. A record name must not contain the keyword `aleo`, and must not be a prefix of any other record name.

Records contain component declarations `{visibility} {name}: {type},`. Names of record components must not contain the keyword `aleo`.

The visibility qualifier may be specified as `constant`, `public`, or `private`. If no qualifier is provided, Leo defaults to `private`.

Record data structures must always contain a component named `owner` of type `address`, as shown below. When passing a record as input to a program function, the `_nonce: group` and `_version: u8` components are also required but do not need to be declared in the Leo program. They are inserted automatically by the compiler.

```leo file=../code_snippets/data_types/demo/src/main.leo#token_record showLineNumbers
```
