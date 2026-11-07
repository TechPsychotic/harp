//! Android helpers: Python cannot find Android's certificates (OpenSSL looks
//! them up by a newer hash than Android's file names), so gather them into
//! one PEM bundle for `SSL_CERT_FILE`.

use std::path::Path;

/// Collects every CERTIFICATE block from the PEM files in `dir` into `out`.
/// Returns how many were written.
pub fn write_ca_bundle(dir: &Path, out: &Path) -> std::io::Result<usize> {
    let mut bundle = String::new();
    let mut n = 0;
    let mut names: Vec<_> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
    names.sort();
    for p in names {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let mut rest = text.as_str();
        while let Some(a) = rest.find("-----BEGIN CERTIFICATE-----") {
            let Some(b) = rest[a..].find("-----END CERTIFICATE-----") else { break };
            let end = a + b + "-----END CERTIFICATE-----".len();
            bundle.push_str(&rest[a..end]);
            bundle.push('\n');
            n += 1;
            rest = &rest[end..];
        }
    }
    if n == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no certificates found"));
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = out.with_extension("tmp");
    std::fs::write(&tmp, bundle)?;
    std::fs::rename(tmp, out)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gathers_certificates_ignoring_trailing_text() {
        let d = std::env::temp_dir().join(format!("harp-ca-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let cert = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        std::fs::write(d.join("abcd1234.0"), format!("{cert}Certificate:\n  Data: text dump\n")).unwrap();
        std::fs::write(d.join("ef.0"), format!("junk\n{cert}")).unwrap();
        let out = d.join("out/ca.pem");
        assert_eq!(write_ca_bundle(&d, &out).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(out).unwrap().matches("BEGIN CERTIFICATE").count(), 2);
        let empty = d.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(write_ca_bundle(&empty, &d.join("x.pem")).is_err());
    }
}
