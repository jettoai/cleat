use super::device_name;

/// One paired Bluetooth device, as the reclaim rule sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothHeadset {
    pub name: String,
    /// Colon separated and upper case (`70:F9:4A:B6:0C:C9`).
    pub address: String,
    pub is_connected: bool,
}

impl BluetoothHeadset {
    /// `70-f9-4a-b6-0c-c9` to `70:F9:4A:B6:0C:C9`; anything already in that form is unchanged.
    pub fn canonical_address(value: &str) -> String {
        value.replace('-', ":").to_uppercase()
    }

    /// True when one of these config entries names this headset, by name or by address.
    pub fn is_listed(&self, entries: &[String]) -> bool {
        entries.iter().any(|entry| {
            device_name::matches(entry, &self.name, &self.address)
                || Self::canonical_address(entry) == self.address
        })
    }
}
