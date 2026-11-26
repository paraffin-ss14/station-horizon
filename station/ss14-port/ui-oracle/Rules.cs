// Extracted SS14 UI branches, MIT, Copyright (c) 2017-2026 Space Wizards Federation.
namespace StationOracle;
public static class Rules {
    // VerbMenuUIController.AddVerbCategory: empty categories without extra content are omitted.
    public static bool ShowCategory(int count, bool extra) { return count != 0 || extra; }
    // VerbMenuElement.TooltipSupplier: adapter maps nullable Message to hasMessage + message.
    public static string TooltipText(bool hasMessage, string message, string text) { return hasMessage ? message : text; }
}
