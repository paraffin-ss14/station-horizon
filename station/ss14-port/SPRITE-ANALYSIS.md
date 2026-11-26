# Rust migration plan: sprite-oracle (C#)

1 package, 1 source file (129 lines), 0 test files, 0 UI files, grouped into 1 unit.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 1 | 0 | 0 | 0 |

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `Rules.cs` | 129 | 68 | 83 | 0 | P/Invoke (C ABI) | Callback-shaped APIs (2); Null references (2); Dynamic typing (1) |

### `Rules.cs`

129 lines in 1 file, wave 0, priority 68, readiness 83, bridge **P/Invoke (C ABI)**.

- Leaf: imports no other unit.
- No tests found: write characterization tests against the original code first.
- Callback-shaped APIs x2: `Rules.cs:11`, `Rules.cs:11`
- Null references x2: `Rules.cs:10`, `Rules.cs:11`
- Dynamic typing x1: `Rules.cs:54`
- Integer width and overflow x1: `Rules.cs:54`
- Division and modulo x1: `Rules.cs:147`
- Exceptions x2: `Rules.cs:9`, `Rules.cs:9`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. Oracle (`.`, portable, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `Rules.cs` | ready | 129 | 83 | P/Invoke (C ABI) | Callback-shaped APIs (2); Null references (2) |

## Risk guide

- **Dynamic typing** (Medium): Values whose shape is checked at runtime need an explicit Rust enum or `serde_json::Value`.
- **Exceptions** (Low): Thrown errors become `Result`. Errors crossing the bridge must map back to the same JS error shape.
- **Callback-shaped APIs** (Medium): Function-typed parameters and methods cross the bridge on every call. Keep boundaries coarse: pass data in, get data out.
- **Integer width and overflow** (Medium): Python ints never overflow; C, Go and Java ints wrap or are undefined on overflow. Pick i64, i128 or a bignum crate deliberately, and use wrapping_* where the source wraps.
- **Division and modulo** (Medium): Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined.
- **Null references** (Medium): Any reference may be null in the source: decide per parameter whether it is `Option` and what null means, instead of mapping every value to `Option`.
