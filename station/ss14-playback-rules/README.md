# SS14 sprite animation cursor

Rust port of RobustToolbox SpriteComponent.Layer.AdvanceFrameAnimation.
See ../ss14-port/PLAYBACK.md for original revision, line attribution and MIT license.
The game directly includes Cursor; generated C ABI and .NET are verification-only.

Run ../ss14-port/verify-playback.ps1 -RustAnything <path/to/rust-anything.exe>.
425/425 C# oracle cases match, including exact f32 bits. Full gate report:
../ss14-port/PLAYBACK-GATE.json. The original source is retained under
../ss14-port/reference/Sprite-AdvanceFrameAnimation.cs.txt.

Cursor expects validated positive delays, at least two frames and a valid frame
index. The public JSON adapter validates its input; the renderer supplies validated
RSI timelines and bypasses static states. This crate does not implement an ECS.
