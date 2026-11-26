// Copyright (c) 2019 Space Station 14 Contributors. MIT: ../ROBUST-LICENSE.txt.
namespace StationOracle;
public static class Rules {
    // LightLevelSystem.GetColourFromLight: point light, CurveFactor=1, Falloff=1.
    // Scalar adapter preserves the original single-precision arithmetic.
    public static double Attenuation(double squaredDistance, double radius) {
        float distance = (float)squaredDistance;
        float range = (float)radius;
        var s = System.Math.Clamp(System.MathF.Sqrt(distance + 1.0f) / range, 0.0f, 1.0f);
        var s2 = s * s;
        var curve = s + (s2 - s) * 1.0f;
        return System.Math.Clamp(((1.0f - s2) * (1.0f - s2)) / (1.0f + curve), 0.0f, 1.0f);
    }
}
