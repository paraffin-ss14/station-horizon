# SS14 RSI FoldDelays

Rust port of RobustToolbox RSIResource.FoldDelays, verified with rust-anything 3.3.0.
Source revision: 1afd6929b473dd537b42cb668a961c84a2e2e7c6.
Source file: Robust.Client/ResourceManagement/ResourceTypes/RSIResource.cs.
Method introduced by Pieter-Jan Briers in 3c08f56619 (2019-12-24), MIT.
Original excerpt and license: ../ss14-port/reference/RSI-FoldDelays.cs.txt,
../ss14-port/ROBUST-LICENSE.txt.

The game includes src/port.rs directly. The generated C ABI bridge and .NET
oracle are development-only. The adapter accepts 1/4/8 direction timelines,
1–64 frames each, finite delays 0.001–60 seconds. It returns a JSON string
containing IEEE float delay bits and frame indices; invalid input returns
`invalid`. Multi-direction folding follows integer millisecond quantization.

Run ../ss14-port/verify-sprite.ps1 -RustAnything <path/to/rust-anything.exe>.
Gate passed 358/358 recorded cases and formatting, Clippy, tests and bridge checks.
Full report: ../ss14-port/SPRITE-GATE.json. Scope: ../SPRITES.md.
This is not a port of the entire SpriteComponent or Robust renderer.
