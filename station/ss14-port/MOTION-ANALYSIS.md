# Rust migration plan: motion-oracle (C#)

1 package, 1 source file (18 lines), 0 test files, 0 UI files, grouped into 1 unit.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 1 | 0 | 0 | 0 |

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `Rules.cs` | 18 | 47 | 89 | 0 | P/Invoke (C ABI) | Dynamic typing (3); Integer width and overflow (3); Division and modulo (1) |

### `Rules.cs`

18 lines in 1 file, wave 0, priority 47, readiness 89, bridge **P/Invoke (C ABI)**.

- Leaf: imports no other unit.
- No tests found: write characterization tests against the original code first.
- Dynamic typing x3: `Rules.cs:6`, `Rules.cs:12`, `Rules.cs:12`
- Integer width and overflow x3: `Rules.cs:8`, `Rules.cs:8`, `Rules.cs:16`
- Division and modulo x1: `Rules.cs:16`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. Oracle (`.`, portable, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `Rules.cs` | ready | 18 | 89 | P/Invoke (C ABI) | Dynamic typing (3); Integer width and overflow (3) |

## Risk guide

- **Dynamic typing** (Medium): Values whose shape is checked at runtime need an explicit Rust enum or `serde_json::Value`.
- **Integer width and overflow** (Medium): Python ints never overflow; C, Go and Java ints wrap or are undefined on overflow. Pick i64, i128 or a bignum crate deliberately, and use wrapping_* where the source wraps.
- **Division and modulo** (Medium): Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined.
