// Copyright (c) 2017-2026 Space Wizards Federation. MIT, ../SS14-LICENSE.txt.
namespace StationOracle;
public static class Rules {
    // AreAccessTagsAllowed; encode up to eight tags and two alternative groups.
    public static bool TagsAllowed(long tags, long denied, long first, long second, int groups) {
        static HashSet<int> Set(long mask) => Enumerable.Range(0, 8).Where(i => (mask & (1L << i)) != 0).ToHashSet();
        var accessTags = Set(tags);
        var denyTags = Set(denied);
        var accessLists = new List<HashSet<int>>();
        if (groups > 0) accessLists.Add(Set(first));
        if (groups > 1) accessLists.Add(Set(second));
        if (denyTags.Overlaps(accessTags)) return false;
        if (accessLists.Count == 0) return true;
        foreach (var set in accessLists) {
            if (set.IsSubsetOf(accessTags)) return true;
        }
        return false;
    }
}
