//! System Integrity Protection (SIP) awareness

use std::path::Path;

/// Check if System Integrity Protection is enabled
#[cfg(target_os = "macos")]
pub fn is_sip_enabled() -> bool {
    // Try to check SIP status via csrutil
    std::process::Command::new("csrutil")
        .arg("status")
        .output()
        .map(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("enabled")
        })
        .unwrap_or(true) // Assume enabled if we can't check
}

#[cfg(not(target_os = "macos"))]
pub fn is_sip_enabled() -> bool {
    false
}

/// Paths protected by SIP
const SIP_PROTECTED_PATHS: &[&str] = &[
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/var",
    "/private/var",
];

/// Check if a path is protected by SIP
pub fn is_sip_protected(path: &Path) -> bool {
    if !is_sip_enabled() {
        return false;
    }

    let path_str = path.to_string_lossy();
    SIP_PROTECTED_PATHS
        .iter()
        .any(|protected| path_str.starts_with(protected))
}

/// Get a list of paths that are safe to use for containers
pub fn get_safe_paths() -> Vec<&'static str> {
    vec![
        "/tmp",
        "/private/tmp",
        "/var/folders",
        "/Users",
        "/Volumes",
        "/Library",
        "/opt",
        "/usr/local",
    ]
}

/// Check if we can write to a path considering SIP
pub fn can_write_to(path: &Path) -> bool {
    if is_sip_protected(path) {
        return false;
    }

    // Check actual write permissions
    if let Ok(metadata) = path.metadata() {
        !metadata.permissions().readonly()
    } else if let Some(parent) = path.parent() {
        // If path doesn't exist, check parent
        if parent.exists() {
            std::fs::metadata(parent)
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        false
    }
}

/// Get information about SIP status
pub fn get_sip_info() -> SipInfo {
    SipInfo {
        enabled: is_sip_enabled(),
        protected_paths: SIP_PROTECTED_PATHS.iter().map(|s| s.to_string()).collect(),
    }
}

/// SIP status information
#[derive(Debug, Clone)]
pub struct SipInfo {
    pub enabled: bool,
    pub protected_paths: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_sip_protected() {
        // These paths should be protected when SIP is enabled
        assert!(is_sip_protected(Path::new("/System/Library")));
        assert!(is_sip_protected(Path::new("/usr/bin")));

        // These paths should not be protected
        assert!(!is_sip_protected(Path::new("/tmp")));
        assert!(!is_sip_protected(Path::new("/Users")));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_sip_protected() {
        // SIP is not available on non-macOS platforms
        assert!(!is_sip_protected(Path::new("/System/Library")));
        assert!(!is_sip_protected(Path::new("/usr/bin")));
    }

    #[test]
    fn test_get_safe_paths() {
        let safe = get_safe_paths();
        assert!(safe.contains(&"/tmp"));
        assert!(safe.contains(&"/Users"));
    }
}
