# SS14 atmosphere guards

Source: https://github.com/space-wizards/space-station-14/blob/f5b48b6547a1f9bfd1f1613d879d68064e22f57f/Content.Server/Atmos/EntitySystems/AtmosphereSystem.GridAtmosphere.cs
MIT, Copyright (c) 2017-2026 Space Wizards Federation. License: ../ss14-port/SS14-LICENSE.txt.

Extracted UpdateAdjacentTiles directional blocker guard and GetDefaultMapAtmosphere IsSpace flag. Gas mixtures/ECS and complete atmosphere solver excluded. Original source and executable adapter: ../ss14-port/rooms-oracle/.

Generated with the local rust-anything build, Rust port manually implemented, C# oracle recorded, build/gate passed (16/16 cases). The station room graph is separate new Rust code. See ../ROOMS.md for exact scope and verification.
