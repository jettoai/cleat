import Foundation

/// Matching a config entry against a device.
///
/// Device names are not typeable: the Maono capture device carries a NO-BREAK SPACE (U+00A0)
/// inside its name, which copies and pastes as an ordinary space and then compares unequal
/// forever. So a name is compared after normalisation, and a UID - which is exact by
/// construction - is compared literally, letting two identically named devices be told apart.
enum DeviceName {

    /// Space characters that render as a plain space but are not one.
    private static let spaceLike: Set<Character> = [
        "\u{00A0}",  // NO-BREAK SPACE (Maono)
        "\u{202F}",  // NARROW NO-BREAK SPACE
        "\u{2007}",  // FIGURE SPACE
        "\u{2009}",  // THIN SPACE
        "\u{200A}"   // HAIR SPACE
    ]

    /// NFKC, then every space-like character to a plain space, then trim and collapse runs.
    /// Case is deliberately preserved: "brio 100" is a typo, not a match.
    static func normalize(_ value: String) -> String {
        let folded = value.precomposedStringWithCompatibilityMapping
        var mapped = String()
        mapped.reserveCapacity(folded.count)
        for character in folded {
            mapped.append(spaceLike.contains(character) ? " " : character)
        }
        return mapped.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
    }

    /// True when a config entry names this device, either by normalised name or by exact UID.
    static func matches(entry: String, name: String, uid: String) -> Bool {
        if !entry.isEmpty, entry == uid { return true }
        return normalize(entry) == normalize(name)
    }
}
