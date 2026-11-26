// Adapted, dependency-free characterization of the linked SS14 branches.
// Copyright (c) 2017-2026 Space Wizards Federation. MIT; ../SS14-LICENSE.txt.
namespace StationOracle;
public static class Rules {
    // InventorySystem.Equip.cs CanEquip: pure slot/size branch only, not ECS event checks.
    public static bool CanFit(int slot, int clothing, bool hasClothing, bool hasItem, int size, int small) {
        bool fittingInPocket = (slot & (1 << 12)) == (1 << 12) && hasItem && size <= small;
        if (!hasClothing && !fittingInPocket || hasClothing && (clothing & slot) != slot && !fittingInPocket)
            return false;
        return true;
    }
    // OnUseSlot dispatch: 3 InteractUsing, 2 unequip, 1 equip, 0 no-op.
    public static int SlotAction(bool held, bool occupied) {
        if (held && occupied) return 3;
        if (occupied) return 2;
        if (!held) return 0;
        return 1;
    }
    // Verb.CompareTo first two branches. Text/culture ordering stays outside this unit.
    public static int Priority(int type, int otherType, int priority, int otherPriority) {
        if (type != otherType) return unchecked(otherType - type);
        if (priority != otherPriority) return unchecked(otherPriority - priority);
        return 0;
    }
    // SharedHandsSystem.SwapHands index branch, without EntitySystem dependencies.
    public static int NextHand(int current, int count, bool reverse) {
        if (count < 2) return current;
        return unchecked(current + (reverse ? -1 : 1) + count) % count;
    }
}
