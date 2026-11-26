// SS14 AtmosphereSystem.GridAtmosphere.cs, MIT, Space Wizards Federation.
namespace StationOracle;
public static class Rules {
    // UpdateAdjacentTiles: directional flags resolved to booleans by the adapter.
    public static bool EdgeOpen(bool ownBlocked, bool adjacentBlocked) {
        if (adjacentBlocked || ownBlocked) return false;
        return true;
    }
    // GetDefaultMapAtmosphere: only the returned IsSpace flag, not gas mixture.
    public static bool DefaultSpace(bool hasMap, bool mapSpace) {
        if (!hasMap) return true;
        return mapSpace;
    }
}
