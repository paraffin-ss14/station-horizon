# Floor textures from SS14

Source revision: f5b48b6547a1f9bfd1f1613d879d68064e22f57f
https://github.com/space-wizards/space-station-14/tree/f5b48b6547a1f9bfd1f1613d879d68064e22f57f/Resources/Textures/Tiles

PNG files are unmodified; hashes and original paths are in manifest.json.
RSI metadata is our adapter, not an upstream RSI file. Variants are static tiles,
not animation. See upstream attribution YAML copies in this folder.

- steel.png (Basic/Gray/gray.png), tech.png (Basic/Gray/gray-maint.png),
  plating.png and grating.png (grating-maint.png): AftrLite, CC0-1.0.
  https://creativecommons.org/publicdomain/zero/1.0/
- damaged.png (Basic/Gray/gray-damaged.png): gray tile by AftrLite, damage overlays
  by WJohn, CC-BY-SA-3.0. https://creativecommons.org/licenses/by-sa/3.0/
  Source: https://github.com/wall-nerds/wallening/blob/9428d97a4fadf8a486b0c6fbe2ee345a2780e687/icons/turf/damaged.dmi

Tile mapping follows SS14 FloorSteel, FloorTechMaint2, FloorTechMaint3 and Plating
prototypes. Our transition strips and deterministic variant selection are local
implementations; no YAML loader or complete SS14 tile system is included.
