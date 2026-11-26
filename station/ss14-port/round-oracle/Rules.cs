// Copyright (c) 2017-2026 Space Wizards Federation. MIT, ../SS14-LICENSE.txt.
namespace StationOracle;
public static class Rules {
    // ToggleReady guards, with ECS/session lookups passed as booleans.
    public static bool CanReady(bool known, bool loaded, bool lobby) {
        if (!known) return false;
        if (!loaded) return false;
        if (!lobby) return false;
        return true;
    }
    // StartRound player selection; session lookup passed as a boolean.
    public static bool CanSpawn(bool lobbyEnabled, bool ready, bool connected) {
        if (lobbyEnabled && !ready) return false;
        if (!connected) return false;
        return true;
    }
}
