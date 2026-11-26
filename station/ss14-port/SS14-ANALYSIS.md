# Rust migration plan: ss14 (C#)

2 packages, 122 source files (11499 lines), 0 test files, 0 UI files, grouped into 25 units.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 15 | 10 | 0 | 0 |

## Start here

No unit exports anything a bridge can carry yet: the units below expose constants, types or nothing a port could call.

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. Content.Client (`Content.Client`, portable, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `Content.Client/AssemblyInfo.cs` | ready | 3 | 100 | P/Invoke (C ABI) | - |
| 0 | `Content.Client/GlobalUsings.cs` | ready | 11 | 96 | P/Invoke (C ABI) | - |
| 0 | `Content.Client/Program.cs` | ready | 12 | 96 | P/Invoke (C ABI) | - |
| 0 | `Content.Client/Hands/UI/` | ready | 12 | 92 | P/Invoke (C ABI) | Class inheritance (1) |
| 2 | `Content.Client/Verbs/` | ready | 202 | 59 | P/Invoke (C ABI) | Dynamic typing (3); Null references (3) |
| 3 | `Content.Client/Verbs/UI/` | caution | 365 | 50 | P/Invoke (C ABI) | Null references (23); Dynamic typing (4) |
| 3 | `Content.Client/ContextMenu/UI/` | caution | 897 | 7 | P/Invoke (C ABI) | String offsets and slicing (1); Null references (37) |
| 4 | `Content.Client/Hands/Systems/` | caution | 339 | 50 | P/Invoke (C ABI) | Null references (16); Reflection and prototypes (11) |
| 5 | `Content.Client/Hands/` | ready | 75 | 71 | P/Invoke (C ABI) | Division and modulo (2); Null references (2) |
| 5 | `Content.Client/Inventory/` | caution | 616 | 22 | P/Invoke (C ABI) | String offsets and slicing (1); Null references (22) |

### 2. Content.Shared (`Content.Shared`, portable, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `Content.Shared/GlobalUsings.cs` | ready | 12 | 96 | P/Invoke (C ABI) | - |
| 0 | `Content.Shared/SharedArrayExtension.cs` | caution | 25 | 93 | P/Invoke (C ABI) | Null references (1) |
| 0 | `Content.Shared/Interaction/Components/` | ready | 142 | 80 | P/Invoke (C ABI) | Class inheritance (7) |
| 0 | `Content.Shared/Inventory/Events/` | ready | 301 | 70 | P/Invoke (C ABI) | Class inheritance (24) |
| 0 | `Content.Shared/Interaction/Events/` | ready | 213 | 65 | P/Invoke (C ABI) | Class inheritance (11); Null references (3) |
| 1 | `Content.Shared/Inventory/VirtualItem/` | ready | 275 | 68 | P/Invoke (C ABI) | Null references (7); Class inheritance (2) |
| 1 | `Content.Shared/Item/ItemToggle/Components/` | ready | 336 | 68 | P/Invoke (C ABI) | Class inheritance (9); Null references (6) |
| 1 | `Content.Shared/Hands/` | ready | 315 | 67 | P/Invoke (C ABI) | Class inheritance (21) |
| 1 | `Content.Shared/Item/ItemToggle/` | ready | 392 | 63 | P/Invoke (C ABI) | Null references (22); Class inheritance (3) |
| 1 | `Content.Shared/Hands/Components/` | ready | 228 | 61 | P/Invoke (C ABI) | Null references (10); Class inheritance (6) |
| 1 | `Content.Shared/Verbs/` | caution | 689 | 38 | P/Invoke (C ABI) | Null references (15); Class inheritance (14) |
| 1 | `Content.Shared/Item/` | caution | 911 | 26 | P/Invoke (C ABI) | String offsets and slicing (1); Null references (26) |
| 1 | `Content.Shared/Hands/EntitySystems/` | caution | 1427 | 0 | P/Invoke (C ABI) | String offsets and slicing (6); Null references (81) |
| 1 | `Content.Shared/Interaction/` | caution | 2194 | 0 | P/Invoke (C ABI) | String offsets and slicing (1); Null references (70) |
| 1 | `Content.Shared/Inventory/` | caution | 1507 | 0 | P/Invoke (C ABI) | Hash iteration order (3); String offsets and slicing (2) |

## NuGet dependencies

| Package | Units | Rust | Fit | Note |
| --- | ---: | --- | --- | --- |
| `Robust.Shared` | 22 | ? | unknown |  |
| `Content.Shared` | 15 | ? | unknown |  |
| `Robust.Client` | 8 | ? | unknown |  |
| `JetBrains.Annotations` | 7 | ? | unknown |  |
| `Content.Client` | 5 | ? | unknown |  |

## Risk guide

- **String offsets and slicing** (High): JS indexes strings in UTF-16 code units, Rust in UTF-8 bytes. Offsets that cross the boundary must be converted, or non-ASCII text gets wrong ranges silently.
- **Dynamic typing** (Medium): Values whose shape is checked at runtime need an explicit Rust enum or `serde_json::Value`.
- **Exceptions** (Low): Thrown errors become `Result`. Errors crossing the bridge must map back to the same JS error shape.
- **Generators** (Medium): Lazy generators become iterators, or return a collected Vec across the bridge.
- **Class inheritance** (Medium): Class hierarchies become traits plus composition; `instanceof` checks become enums.
- **Reflection and prototypes** (Medium): Proxy, Reflect, Symbol and prototype tricks need a redesign, not a translation.
- **Callback-shaped APIs** (Medium): Function-typed parameters and methods cross the bridge on every call. Keep boundaries coarse: pass data in, get data out.
- **Integer width and overflow** (Medium): Python ints never overflow; C, Go and Java ints wrap or are undefined on overflow. Pick i64, i128 or a bignum crate deliberately, and use wrapping_* where the source wraps.
- **Division and modulo** (Medium): Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined.
- **Hash iteration order** (High): Set and map iteration order comes from the source language's hash function. A port cannot reproduce it: sort before output, or make the order part of the contract.
- **Concurrency** (Medium): Threads, goroutines and locks need a design in Rust (channels, Send/Sync), not a line-by-line translation; keep concurrency in the caller where possible.
- **Null references** (Medium): Any reference may be null in the source: decide per parameter whether it is `Option` and what null means, instead of mapping every value to `Option`.
