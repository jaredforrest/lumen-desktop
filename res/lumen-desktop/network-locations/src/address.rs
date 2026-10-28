use glib::{Uri, UriFlags};
use std::net::Ipv6Addr;

pub const PROTOCOLS: [&str; 8] = ["smb", "sftp", "dav", "davs", "ftp", "ftps", "afp", "nfs"];

pub fn address(value: &str, strip_identity: bool) -> Result<String, &'static str> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(|ch| ch < ' ') {
        return Err("Enter a server address of at most 4096 bytes");
    }
    let parsed = Uri::parse(value, UriFlags::HAS_PASSWORD | UriFlags::ENCODED_PATH)
        .map_err(|_| "Enter a valid server address")?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if !PROTOCOLS.contains(&scheme.as_str()) { return Err("Choose a supported network protocol"); }
    if !strip_identity && parsed.userinfo().is_some() {
        return Err("Do not include a username or password in the address; the system dialog will ask when needed");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("Server addresses cannot contain a query or fragment");
    }
    let raw_host = parsed.host().ok_or("Enter a valid server hostname")?;
    if raw_host.is_empty() || raw_host.contains(['%', '\\']) || raw_host.chars().any(char::is_whitespace) {
        return Err("Enter a valid server hostname");
    }
    let host = if raw_host.contains(':') {
        format!("[{}]", raw_host.parse::<Ipv6Addr>()
            .map_err(|_| "Enter a valid hostname and port")?)
    } else {
        let ascii = glib::hostname_to_ascii(&raw_host).ok_or("Enter a valid hostname and port")?;
        let ascii = ascii.to_ascii_lowercase();
        if !ascii.bytes().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || b"_.-".contains(&ch)) {
            return Err("Enter a valid hostname and port");
        }
        ascii
    };
    let port = parsed.port();
    if port == 0 || port > 65535 { return Err("Enter a valid hostname and port"); }
    let path = parsed.path();
    let path = if path.is_empty() { "/" } else { path.as_str() };
    let bytes = path.as_bytes();
    for (index, ch) in bytes.iter().enumerate() {
        if *ch == b'%' && (index + 2 >= bytes.len() || !bytes[index + 1].is_ascii_hexdigit() || !bytes[index + 2].is_ascii_hexdigit()) {
            return Err("The server path has an invalid escape");
        }
    }
    let mut escaped = String::new();
    for ch in bytes {
        if ch.is_ascii_alphanumeric() || b"/:@!$&'()*+,;=-._~%".contains(ch) { escaped.push(*ch as char); }
        else { escaped.push_str(&format!("%{ch:02X}")); }
    }
    Ok(format!("{scheme}://{host}{}{escaped}", if port > 0 { format!(":{port}") } else { String::new() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_credentials_and_ambiguous_addresses() {
        assert_eq!(address("SFTP://Example.COM/a b", false).unwrap(), "sftp://example.com/a%20b");
        assert_eq!(address("sftp://user:hidden-secret@server/path", true).unwrap(), "sftp://server/path");
        for input in ["sftp://user:hidden-secret@server/path", "sftp://server/?token=hidden-secret",
            "sftp://server/#hidden-secret", "sftp://user%3Ahidden-secret%40server/path",
            "file:///etc", "sftp://server:99999/path", "sftp://[invalid]/", "sftp://server/%xx"] {
            let error = address(input, false).unwrap_err();
            assert!(!error.contains("hidden-secret"));
        }
    }
}
