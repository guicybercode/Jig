/// Largest accepted dotted method name. Catalog names are far smaller.
const MAX_METHOD_BYTES: usize = 128;

/// Returns whether `method` is a dotted identifier the bridge will forward.
///
/// The desktop process does not implement catalog methods. It only rejects
/// strings that cannot be a wire method so a malformed invoke never hits the
/// socket. Unknown catalog names are still forwarded; the daemon answers
/// `method_not_found`.
#[must_use]
pub fn is_wire_method_name(method: &str) -> bool {
    if method.is_empty() || method.len() > MAX_METHOD_BYTES {
        return false;
    }
    let mut parts = method.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if !is_ident(first) {
        return false;
    }
    let mut extra = 0_usize;
    for part in parts {
        if !is_ident(part) {
            return false;
        }
        extra += 1;
    }
    extra >= 1
}

fn is_ident(part: &str) -> bool {
    let mut bytes = part.bytes();
    match bytes.next() {
        Some(byte) if byte.is_ascii_lowercase() => {}
        _ => return false,
    }
    bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::is_wire_method_name;

    #[test]
    fn accepts_catalog_shaped_names() {
        assert!(is_wire_method_name("system.hello"));
        assert!(is_wire_method_name("agent.custom.create"));
        assert!(is_wire_method_name("git.status"));
        assert!(!is_wire_method_name(""));
        assert!(!is_wire_method_name("hello"));
        assert!(!is_wire_method_name("System.hello"));
        assert!(!is_wire_method_name("session."));
        assert!(!is_wire_method_name(".start"));
        assert!(!is_wire_method_name("session.create extra"));
    }
}
