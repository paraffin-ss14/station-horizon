# Rust migration plan: editor-oracle (C#)

1 package, 1 source file (11 lines), 0 test files, 0 UI files, grouped into 1 unit.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 1 | 0 | 0 | 0 |

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `Rules.cs` | 11 | 49 | 94 | 0 | P/Invoke (C ABI) | Division and modulo (2); Integer width and overflow (1) |

### `Rules.cs`

11 lines in 1 file, wave 0, priority 49, readiness 94, bridge **P/Invoke (C ABI)**.

- Leaf: imports no other unit.
- No tests found: write characterization tests against the original code first.
- Division and modulo x2: `Rules.cs:9`, `Rules.cs:9`
- Integer width and overflow x1: `Rules.cs:9`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. Oracle (`.`, portable, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `Rules.cs` | ready | 11 | 94 | P/Invoke (C ABI) | Division and modulo (2); Integer width and overflow (1) |

## Risk guide

- **Integer width and overflow** (Medium): Python ints never overflow; C, Go and Java ints wrap or are undefined on overflow. Pick i64, i128 or a bignum crate deliberately, and use wrapping_* where the source wraps.
- **Division and modulo** (Medium): Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined.

## Selected scope and provenance

Unit-grid rectangle axis count only. The original arithmetic was added in RobustToolbox commit 895cd6e03e01c883c3861b875a9e7643e55dafe6 (2023-01-19), PlacementMode.GridCoordinates, MIT under ROBUST-LEGAL.md. Older placement classes are not copied. C# oracle adapts GridDistancing=1 and integral center differences. Native Rust module is called by station-sim editor; generated bridge is test-only. See ../EDITOR.md.
