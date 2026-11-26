// Copyright (c) 2019 Space Station 14 Contributors. MIT, ../ROBUST-LICENSE.txt.
namespace StationOracle;
public static class Rules {
    // TransformSystem.FrameUpdate: scalar extraction of the original float guard.
    public static bool CanInterpolate(double distanceSquared, bool active, bool sameParent, bool validParent, bool deleted) {
        float distance = (float)distanceSquared;
        return active && sameParent && validParent && !deleted
            && distance > 0.001f * 0.001f && distance < 2.0f * 2.0f;
    }
    // ClientGameStateManager: removing confirmed input sequences, uint inputs.
    public static bool DiscardInput(long sequence, long acknowledged) {
        return (uint)sequence <= (uint)acknowledged;
    }
    // ClientGameStateManager: lagTickCount; scalar double adapter of the formula.
    public static double LagTicks(double tickRate, double pingSeconds, double timeScale) {
        return System.Math.Ceiling(tickRate * pingSeconds / timeScale);
    }
}
