// MIT: Space Station 14 Contributors. See ../ROBUST-LICENSE.txt.
namespace StationOracle;
public static class Rules {
    // PlacementMode.GridCoordinates, added in 895cd6e03e0 (2023-01-19).
    // Scalar adapter: one axis, unit GridDistancing, integral tile-center delta.
    public static double AxisCount(long delta) {
        float difference = delta;
        var sign = System.Math.Sign(difference);
        return System.Math.Ceiling(System.Math.Abs((difference + 1f / 2f * sign) / 1f));
    }
}
