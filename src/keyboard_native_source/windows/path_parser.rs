use crate::keyboard_source::{Keyboard, KeyboardID, PortID};

pub(crate) struct KeyboardPathParser;

impl KeyboardPathParser {
    pub(crate) fn parse(path: &str) -> Keyboard {
        let (vendor_id, product_id, serial) = Self::parse_hid_path(path);

        Keyboard {
            keyboard_id: KeyboardID {
                name: None,
                vendor_id,
                product_id,
                serial,
            },
            port_id: PortID {
                physical_path: Some(path.to_owned()),
            },
        }
    }

    fn parse_hid_path(path: &str) -> (Option<String>, Option<String>, Option<String>) {
        let parts: Vec<&str> = path.split('#').collect();

        // Typical path:
        // \\?\HID#VID_1234&PID_5678&MI_00#INSTANCE#{GUID}
        let hardware_id = parts.get(1).copied().unwrap_or_default();

        let vendor_id = Self::extract_hex(hardware_id, "VID_");
        let product_id = Self::extract_hex(hardware_id, "PID_");

        // Do NOT use the Windows instance ID as the serial number.
        // The real serial (if present) is read later via HidD_GetSerialNumberString.
        let serial = None;

        (vendor_id, product_id, serial)
    }

    fn extract_hex(value: &str, prefix: &str) -> Option<String> {
        let start = (0..value.len()).find(|&index| {
            value[index..]
                .get(..prefix.len())
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        })?;

        let value = &value[start + prefix.len()..];

        let end = value
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(value.len());

        if end == 0 {
            return None;
        }

        // Normalize to 4-digit lowercase hex for cross-platform consistency
        let hex_str = &value[..end];
        u16::from_str_radix(hex_str, 16)
            .ok()
            .map(|id| format!("{:04x}", id))
    }
}
