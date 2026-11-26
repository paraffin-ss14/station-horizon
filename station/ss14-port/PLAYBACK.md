# Sprite playback provenance and verification

Source: RobustToolbox revision `1afd6929b473dd537b42cb668a961c84a2e2e7c6`,
`Robust.Client/GameObjects/Components/Renderable/SpriteComponent.cs`,
`Layer.AdvanceFrameAnimation`, lines 1699–1764 at this revision.

Original method: `reference/Sprite-AdvanceFrameAnimation.cs.txt`.
Line attribution: `reference/Sprite-AdvanceFrameAnimation.blame.txt`.
The method was introduced/refactored in `d52cbca984` (Leon Friedrich, 2023),
with reverse/cycle logic in `495671576e` (metalgearsloth, 2023) and subsequent
changes. The recorded lines postdate the MIT cutoff in `ROBUST-LEGAL.md`.
Keep `ROBUST-LICENSE.txt` with the port. No SS14 artwork was relicensed.

`playback-oracle/Rules.cs` wraps the unchanged original method with minimal
RSI.State/parent stand-ins and bounded JSON input. Validation permits 2–64
finite delays of 0.001–60 seconds, a valid initial frame, finite remaining
time in [-60,60] and delta in [0,60]. Static one-frame states are handled by
the runtime without invoking this algorithm, as in the upstream animation loop.

`ss14-playback-rules/src/port.rs` reproduces the original f32 operations,
strict negative boundary, reverse direction, loop stop and nonduplicated
ping-pong endpoints. The JSON adapter compares frame, direction, animated
flag and the exact float bits of remaining and animation time.

rust-anything 3.3.0 recorded 425 generated/domain cases: 425 matched. Full
formatting, Clippy, tests, bridge and behavioral gate passed; see
`PLAYBACK-GATE.json` and `PLAYBACK-DIFF.json`. Reproduce with `verify-playback.ps1`.

The game includes Cursor directly and needs no .NET runtime. Station's
surrounding ownership, keyed layer lifetime, manual seek, synchronization,
events and rendering adapters are native integrations, not a claim that all
of RobustToolbox was translated. Large looping seeks reduce whole periods
before invoking Cursor to bound work. They are not part of the C# adapter's
exact-bit comparison domain.
