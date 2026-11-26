# Rust migration plan: playback-oracle (C#)

1 package, 1 source file (75 lines), 0 test files, 0 UI files, grouped into 1 unit.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 1 | 0 | 0 | 0 |

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `Rules.cs` | 75 | 58 | 93 | 0 | P/Invoke (C ABI) | Callback-shaped APIs (1); Null references (1); Exceptions (2) |

### `Rules.cs`

75 lines in 1 file, wave 0, priority 58, readiness 93, bridge **P/Invoke (C ABI)**.

- Leaf: imports no other unit.
- No tests found: write characterization tests against the original code first.
- Callback-shaped APIs x1: `Rules.cs:6`
- Null references x1: `Rules.cs:6`
- Exceptions x2: `Rules.cs:5`, `Rules.cs:5`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. playback-oracle (`.`, portable, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `Rules.cs` | ready | 75 | 93 | P/Invoke (C ABI) | Callback-shaped APIs (1); Null references (1) |

## Risk guide

- **Exceptions** (Low): Thrown errors become `Result`. Errors crossing the bridge must map back to the same JS error shape.
- **Callback-shaped APIs** (Medium): Function-typed parameters and methods cross the bridge on every call. Keep boundaries coarse: pass data in, get data out.
- **Null references** (Medium): Any reference may be null in the source: decide per parameter whether it is `Option` and what null means, instead of mapping every value to `Option`.
