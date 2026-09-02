import Foundation

/// Per-device silence threshold for the liveness rule.
struct LivenessConfig: Codable, Equatable, Sendable {
    /// Seconds of uninterrupted digital silence before the device counts as absent. Must be >= 1.
    var zeroSeconds: Double

    init(zeroSeconds: Double) {
        self.zeroSeconds = zeroSeconds
    }
}

/// A config file that parses but asks for something impossible.
enum ConfigError: Error, Equatable, LocalizedError {
    case balanceOutOfRange(Double)
    case volumeOutOfRange(device: String, value: Double)
    case zeroSecondsTooSmall(device: String, value: Double)

    var errorDescription: String? {
        switch self {
        case .balanceOutOfRange(let value):
            return "balance must be between 0 and 1, got \(value)"
        case .volumeOutOfRange(let device, let value):
            return "inputVolume[\"\(device)\"] must be between 0 and 100, got \(value)"
        case .zeroSecondsTooSmall(let device, let value):
            return "liveness[\"\(device)\"].zeroSeconds must be at least 1, got \(value)"
        }
    }
}

/// The declared state, as written in `~/.config/cleat/config.json`.
///
/// Every field is optional in the file and falls back to a default that turns its rule off, so a
/// half-written config never means "enforce something the user did not ask for".
struct Config: Codable, Equatable, Sendable {
    /// Input device priority, most preferred first. Empty disables the input pin rule.
    var input: [String]
    /// Devices that must never be the default input.
    var blockedInput: [String]
    /// Output device priority, most preferred first. Empty disables the output pin rule.
    var output: [String]
    /// 0.0-1.0, 0.5 is centred. `nil` disables the balance rule.
    var balance: Double?
    /// Per-device input volume, 0-100 percent. Devices not listed are left alone.
    var inputVolume: [String: Double]
    /// Per-device silence detection. Devices not listed are never measured.
    var liveness: [String: LivenessConfig]
    /// Whether the app registers itself with SMAppService as a login item.
    var launchAtLogin: Bool

    /// What Cleat enforces when there is no config file at all: nothing.
    static let disabled = Config()

    init(
        input: [String] = [],
        blockedInput: [String] = [],
        output: [String] = [],
        balance: Double? = nil,
        inputVolume: [String: Double] = [:],
        liveness: [String: LivenessConfig] = [:],
        launchAtLogin: Bool = true
    ) {
        self.input = input
        self.blockedInput = blockedInput
        self.output = output
        self.balance = balance
        self.inputVolume = inputVolume
        self.liveness = liveness
        self.launchAtLogin = launchAtLogin
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        input = try container.decodeIfPresent([String].self, forKey: .input) ?? []
        blockedInput = try container.decodeIfPresent([String].self, forKey: .blockedInput) ?? []
        output = try container.decodeIfPresent([String].self, forKey: .output) ?? []
        balance = try container.decodeIfPresent(Double.self, forKey: .balance)
        inputVolume = try container.decodeIfPresent([String: Double].self, forKey: .inputVolume) ?? [:]
        liveness = try container.decodeIfPresent([String: LivenessConfig].self, forKey: .liveness) ?? [:]
        launchAtLogin = try container.decodeIfPresent(Bool.self, forKey: .launchAtLogin) ?? true
    }

    /// Range checks the decoder cannot express. Called by `load`, so an out-of-range file is
    /// rejected the same way a malformed one is: the previous config stays in force.
    func validate() throws {
        if let balance, balance < 0 || balance > 1 {
            throw ConfigError.balanceOutOfRange(balance)
        }
        for (device, value) in inputVolume where value < 0 || value > 100 {
            throw ConfigError.volumeOutOfRange(device: device, value: value)
        }
        for (device, entry) in liveness where entry.zeroSeconds < 1 {
            throw ConfigError.zeroSecondsTooSmall(device: device, value: entry.zeroSeconds)
        }
    }

    static func load(from url: URL) throws -> Config {
        let data = try Data(contentsOf: url)
        let config = try JSONDecoder().decode(Config.self, from: data)
        try config.validate()
        return config
    }
}
