// SS14 pure branches, MIT, Copyright (c) 2017-2026 Space Wizards Federation.
// ECS lookup, numeric load comparison and grid enumeration supplied by the adapter.
namespace StationOracle;
public static class Rules {
    // PowerNetSystem.IsPoweredCalculate: load comparison passed as enoughPower.
    public static bool ReceiverPowered(bool disabled, bool needsPower, bool positiveLoad, bool enoughPower) {
        if (disabled) return false;
        if (!needsPower) return true;
        return positiveLoad && enoughPower;
    }
    // CableNode.GetReachableNodes: after cardinal/same-tile enumeration.
    public static bool CableReachable(bool anchored, bool hasGrid, bool cable, bool sameTileDevice, bool blockedDirection) {
        if (!anchored || !hasGrid) return false;
        if (blockedDirection) return false;
        return cable || sameTileDevice;
    }
    // SharedAirlockSystem.OnBeforePry.
    public static bool PryAllowed(bool cancelled, bool powered, bool pryPowered) {
        if (cancelled) return false;
        return !powered || pryPowered;
    }
}
