// Pure branches extracted from SS14. Copyright (c) 2017-2026 Space Wizards Federation. MIT.
namespace StationOracle;
public static class Rules {
    // BypassLockSystem.OnInteractUsing: guards only. ECS events/DoAfter excluded.
    public static bool BypassReady(bool sameUser, bool hasQuality, bool locked, bool canForce) {
        if (sameUser) return false;
        if (!hasQuality || !locked) return false;
        if (!canForce) return false;
        return true;
    }
    // SharedStorageSystem.CanInsert: size and nested-storage guard only.
    public static bool StorageSizeFits(int size, int maxSize, bool isStorage, int nestedMax) {
        if (size > maxSize) return false;
        if (isStorage && nestedMax >= maxSize) return false;
        return true;
    }
}
