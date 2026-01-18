//! Connection endpoint parsing
//!
//! Endpoints can be:
//! - Pin references: `"R1:1"`, `"U1:VCC"` (component:pin)
//! - Net labels: `"&GND"`, `"&VCC"`, `"&SDA"` (ampersand prefix)

use std::fmt;

/// A connection endpoint - either a pin on a component or a net label
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConnectionEndpoint {
    /// Pin on a component (reference:pin_number)
    Pin {
        /// Component reference designator (e.g., "R1", "U1")
        reference: String,
        /// Pin number or name (e.g., "1", "VCC")
        pin: String,
    },
    /// Net label (e.g., "GND", "VCC")
    Net(String),
}

impl ConnectionEndpoint {
    /// Parse an endpoint string into a ConnectionEndpoint
    ///
    /// # Format
    /// - `"R1:1"` → Pin { reference: "R1", pin: "1" }
    /// - `"&GND"` → Net("GND")
    ///
    /// # Errors
    /// Returns `ConnectionError::InvalidEndpoint` if the string doesn't match either format
    pub fn parse(s: &str) -> Result<Self, ConnectionError> {
        let s = s.trim();

        if s.is_empty() {
            return Err(ConnectionError::InvalidEndpoint(s.to_string()));
        }

        if let Some(net_name) = s.strip_prefix('&') {
            if net_name.is_empty() {
                return Err(ConnectionError::InvalidEndpoint(s.to_string()));
            }
            Ok(ConnectionEndpoint::Net(net_name.to_string()))
        } else if let Some((reference, pin)) = s.split_once(':') {
            if reference.is_empty() || pin.is_empty() {
                return Err(ConnectionError::InvalidEndpoint(s.to_string()));
            }
            Ok(ConnectionEndpoint::Pin {
                reference: reference.to_string(),
                pin: pin.to_string(),
            })
        } else {
            Err(ConnectionError::InvalidEndpoint(s.to_string()))
        }
    }

    /// Check if this endpoint is a pin
    pub fn is_pin(&self) -> bool {
        matches!(self, ConnectionEndpoint::Pin { .. })
    }

    /// Check if this endpoint is a net label
    pub fn is_net(&self) -> bool {
        matches!(self, ConnectionEndpoint::Net(_))
    }

    /// Get the pin reference if this is a pin endpoint
    pub fn as_pin(&self) -> Option<(&str, &str)> {
        match self {
            ConnectionEndpoint::Pin { reference, pin } => Some((reference, pin)),
            _ => None,
        }
    }

    /// Get the net name if this is a net endpoint
    pub fn as_net(&self) -> Option<&str> {
        match self {
            ConnectionEndpoint::Net(name) => Some(name),
            _ => None,
        }
    }

    /// Check if this is a power net (GND, VCC, +3V3, etc.)
    pub fn is_power_net(&self) -> bool {
        match self {
            ConnectionEndpoint::Net(name) => is_power_net_name(name),
            _ => false,
        }
    }
}

impl fmt::Display for ConnectionEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionEndpoint::Pin { reference, pin } => write!(f, "{}:{}", reference, pin),
            ConnectionEndpoint::Net(name) => write!(f, "&{}", name),
        }
    }
}

/// Check if a net name is a power net
/// Power nets use global labels instead of local labels
pub fn is_power_net_name(name: &str) -> bool {
    let upper = name.to_uppercase();

    // Common ground names
    if matches!(upper.as_str(), "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GNDA" | "GNDD") {
        return true;
    }

    // VCC variants
    if upper.starts_with("VCC") || upper.starts_with("VDD") || upper.starts_with("VSS") {
        return true;
    }

    // Voltage rails like +3V3, +5V, +12V, -5V, etc.
    if (upper.starts_with('+') || upper.starts_with('-')) && upper.contains('V') {
        return true;
    }

    // Common power names
    if matches!(upper.as_str(), "3V3" | "5V" | "12V" | "1V8" | "2V5" | "VBAT" | "VIN" | "VOUT") {
        return true;
    }

    false
}

/// Errors that can occur during connection operations
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionError {
    /// Invalid endpoint format
    InvalidEndpoint(String),
    /// Component not found
    ComponentNotFound(String),
    /// Pin not found on component
    PinNotFound { reference: String, pin: String },
    /// Not enough endpoints (minimum 2 required)
    InsufficientEndpoints,
    /// No connection found between endpoints
    NoConnection,
    /// Other errors
    Other(String),
}

impl fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionError::InvalidEndpoint(s) => {
                write!(f, "Invalid endpoint '{}'. Use 'REF:PIN' for pins or '&NET' for net labels", s)
            }
            ConnectionError::ComponentNotFound(r) => {
                write!(f, "Component '{}' not found", r)
            }
            ConnectionError::PinNotFound { reference, pin } => {
                write!(f, "Pin '{}' not found on component '{}'", pin, reference)
            }
            ConnectionError::InsufficientEndpoints => {
                write!(f, "At least 2 endpoints are required for a connection")
            }
            ConnectionError::NoConnection => {
                write!(f, "No connection found between the specified endpoints")
            }
            ConnectionError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ConnectionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pin_endpoint() {
        let ep = ConnectionEndpoint::parse("R1:1").unwrap();
        assert_eq!(ep, ConnectionEndpoint::Pin {
            reference: "R1".to_string(),
            pin: "1".to_string(),
        });
    }

    #[test]
    fn test_parse_pin_with_name() {
        let ep = ConnectionEndpoint::parse("U1:VCC").unwrap();
        assert_eq!(ep, ConnectionEndpoint::Pin {
            reference: "U1".to_string(),
            pin: "VCC".to_string(),
        });
    }

    #[test]
    fn test_parse_net_endpoint() {
        let ep = ConnectionEndpoint::parse("&GND").unwrap();
        assert_eq!(ep, ConnectionEndpoint::Net("GND".to_string()));
    }

    #[test]
    fn test_parse_power_net() {
        let ep = ConnectionEndpoint::parse("&+3V3").unwrap();
        assert_eq!(ep, ConnectionEndpoint::Net("+3V3".to_string()));
        assert!(ep.is_power_net());
    }

    #[test]
    fn test_parse_invalid_empty() {
        assert!(ConnectionEndpoint::parse("").is_err());
    }

    #[test]
    fn test_parse_invalid_no_colon() {
        assert!(ConnectionEndpoint::parse("R1").is_err());
    }

    #[test]
    fn test_parse_invalid_empty_pin() {
        assert!(ConnectionEndpoint::parse("R1:").is_err());
    }

    #[test]
    fn test_parse_invalid_empty_ref() {
        assert!(ConnectionEndpoint::parse(":1").is_err());
    }

    #[test]
    fn test_parse_invalid_empty_net() {
        assert!(ConnectionEndpoint::parse("&").is_err());
    }

    #[test]
    fn test_is_power_net_name() {
        assert!(is_power_net_name("GND"));
        assert!(is_power_net_name("gnd"));
        assert!(is_power_net_name("VCC"));
        assert!(is_power_net_name("VDD"));
        assert!(is_power_net_name("+3V3"));
        assert!(is_power_net_name("+5V"));
        assert!(is_power_net_name("-12V"));
        assert!(is_power_net_name("VBAT"));

        assert!(!is_power_net_name("SDA"));
        assert!(!is_power_net_name("SCL"));
        assert!(!is_power_net_name("DATA"));
    }

    #[test]
    fn test_display() {
        let pin = ConnectionEndpoint::Pin {
            reference: "R1".to_string(),
            pin: "1".to_string(),
        };
        assert_eq!(pin.to_string(), "R1:1");

        let net = ConnectionEndpoint::Net("GND".to_string());
        assert_eq!(net.to_string(), "&GND");
    }
}
