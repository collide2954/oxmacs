use std::path::{Path, PathBuf};

const HOSTS: [&str; 2] = ["wsl.localhost", "wsl$"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub distro: String,
    pub path: String,
}

pub fn place(path: &Path) -> Option<Place> {
    let text = path.to_string_lossy().replace('/', "\\");
    let rest = text
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| text.strip_prefix(r"\\"))?;
    let (host, rest) = rest.split_once('\\')?;
    if !HOSTS.iter().any(|known| known.eq_ignore_ascii_case(host)) {
        return None;
    }
    let (distro, inside) = rest.split_once('\\').unwrap_or((rest, ""));
    (!distro.is_empty()).then(|| Place {
        distro: distro.to_owned(),
        path: format!("/{}", inside.trim_end_matches('\\').replace('\\', "/")),
    })
}

pub fn share(distro: &str, path: &str) -> PathBuf {
    let inside = path.trim_start_matches('/').replace('/', "\\");
    PathBuf::from(format!(r"\\wsl.localhost\{distro}\{inside}"))
}

pub fn named(text: &str) -> Option<Place> {
    let (distro, path) = text.strip_prefix("wsl:")?.split_once(':')?;
    (!distro.is_empty() && path.starts_with('/')).then(|| Place {
        distro: distro.to_owned(),
        path: path.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(distro: &str, path: &str) -> Option<Place> {
        Some(Place {
            distro: distro.to_owned(),
            path: path.to_owned(),
        })
    }

    #[test]
    fn shares_of_either_host_name_are_places_in_a_distro() {
        assert_eq!(
            place(Path::new(r"\\wsl.localhost\Fedora\home\ana\src\main.rs")),
            at("Fedora", "/home/ana/src/main.rs")
        );
        assert_eq!(
            place(Path::new(r"\\wsl$\Ubuntu-24.04\etc\")),
            at("Ubuntu-24.04", "/etc")
        );
        assert_eq!(
            place(Path::new(r"\\?\UNC\WSL.LOCALHOST\Fedora\tmp")),
            at("Fedora", "/tmp")
        );
        assert_eq!(
            place(Path::new(r"\\wsl.localhost\Fedora")),
            at("Fedora", "/")
        );
        assert_eq!(
            place(Path::new("//wsl.localhost/Fedora/x y")),
            at("Fedora", "/x y")
        );
        assert_eq!(place(Path::new(r"\\server\share\x")), None);
        assert_eq!(place(Path::new(r"C:\Users\ana")), None);
        assert_eq!(place(Path::new("/home/ana")), None);
    }

    #[test]
    fn places_turn_back_into_share_paths() {
        assert_eq!(
            share("Fedora", "/home/ana/src"),
            PathBuf::from(r"\\wsl.localhost\Fedora\home\ana\src")
        );
        assert_eq!(
            share("Fedora", "/"),
            PathBuf::from(r"\\wsl.localhost\Fedora\")
        );
        assert_eq!(
            place(&share("Fedora", "/home/ana")),
            at("Fedora", "/home/ana")
        );
    }

    #[test]
    fn wsl_names_reach_any_distro() {
        assert_eq!(named("wsl:Fedora:/etc/hosts"), at("Fedora", "/etc/hosts"));
        assert_eq!(named("wsl:Fedora:etc"), None);
        assert_eq!(named("wsl::/etc"), None);
        assert_eq!(named(r"C:\x"), None);
    }
}
