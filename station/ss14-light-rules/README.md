# SS14 point-light attenuation

MIT scalar port, CurveFactor=1 / Falloff=1, of RobustToolbox LightLevelSystem.GetColourFromLight. Copyright (c) 2019 Space Station 14 Contributors. See ../ss14-port/ROBUST-LICENSE.txt and ../LIGHTING.md for provenance and limits.

285/285 oracle cases match; rust-anything gate passed. The JSON/FFI bridge is only for verification, and its measured overhead is slower than C#. The station renderer imports src/port.rs directly.
