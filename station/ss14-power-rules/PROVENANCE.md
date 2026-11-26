# SS14 power rule extraction

Source: https://github.com/space-wizards/space-station-14/tree/f5b48b6547a1f9bfd1f1613d879d68064e22f57f
MIT, Copyright (c) 2017-2026 Space Wizards Federation. License: ../ss14-port/SS14-LICENSE.txt.

Extracted guards only, not the complete power solver. Exact upstream references: ../ss14-port/power-oracle/upstream/*.cs.txt. C# adapter: ../ss14-port/power-oracle/Rules.cs. Full boundary and integration description: ../POWER.md.

Generated bridge with local rust-anything; implemented Rust src/port.rs; recorded 112 C# cases including exhaustive boolean domains, built and gated. Reports: ../ss14-port/POWER-GATE.json and POWER-DIFF.json. No claim of speedup.
