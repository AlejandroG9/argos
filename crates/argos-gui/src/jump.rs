use std::process::Command;

const PREFIX: &str = "warp://session/";

/// La URL sale del entorno de un proceso ajeno, así que se valida antes de
/// pasarla a `open`: solo el prefijo exacto y un identificador hexadecimal.
pub fn is_valid_warp_url(url: &str) -> bool {
    let Some(id) = url.strip_prefix(PREFIX) else {
        return false;
    };
    !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn jump_to(url: &str) -> Result<(), std::io::Error> {
    if !is_valid_warp_url(url) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "URL de Warp no válida",
        ));
    }

    Command::new("open").arg(url).status()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acepta_una_url_de_sesion_de_warp() {
        assert!(is_valid_warp_url(
            "warp://session/15afe4b093924586a06cdcced8e499dd"
        ));
    }

    /// El valor viene del entorno de un proceso ajeno: no se le pasa nada
    /// a `open` sin validarlo antes.
    #[test]
    fn rechaza_cualquier_otro_esquema() {
        assert!(!is_valid_warp_url("file:///etc/passwd"));
        assert!(!is_valid_warp_url("https://ejemplo.com"));
        assert!(!is_valid_warp_url("warp://otra-cosa/x"));
        assert!(!is_valid_warp_url(""));
        assert!(!is_valid_warp_url("; rm -rf /"));
    }

    #[test]
    fn rechaza_un_uuid_con_caracteres_no_hexadecimales() {
        assert!(!is_valid_warp_url("warp://session/../../etc"));
        assert!(!is_valid_warp_url("warp://session/abc def"));
    }
}
