// Copyright (c) 2017-2026 Space Wizards Federation. MIT, ../SS14-LICENSE.txt.
namespace StationOracle;
public static class Rules {
    // SharedDoAfterSystem.ShouldCancel hand checks only; NeedHand and hand existence assumed.
    public static bool HandsCancel(bool breakOnDrop, bool initialItem, bool holding, bool breakOnChange, int initialHand, int activeHand) {
        if (breakOnDrop && initialItem && !holding) return true;
        if (breakOnChange && activeHand != initialHand) return true;
        return false;
    }
}
