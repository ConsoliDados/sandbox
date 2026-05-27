//! Locate host dotfiles to bind-mount RO into the container.
//!
//! `~/.config/sandbox/zsh/.zshrc.sandbox` (sandbox-specific) wins over
//! `~/.zshrc` (host). `~/.config/starship.toml` is mounted if present.

use std::path::PathBuf;

use sandbox_core::Paths;

#[derive(Debug, Default)]
pub(crate) struct Dotfiles {
    pub(crate) zshrc: Option<PathBuf>,
    pub(crate) starship: Option<PathBuf>,
}

pub(crate) fn discover(paths: &Paths) -> Dotfiles {
    Dotfiles {
        zshrc: locate_zshrc(paths),
        starship: locate_starship(),
    }
}

fn locate_zshrc(paths: &Paths) -> Option<PathBuf> {
    let sandbox_zshrc = paths.user_zshrc_sandbox();
    if sandbox_zshrc.is_file() {
        return Some(sandbox_zshrc);
    }
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    let zshrc = home.join(".zshrc");
    zshrc.is_file().then_some(zshrc)
}

fn locate_starship() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    let p = dirs.config_dir().join("starship.toml");
    p.is_file().then_some(p)
}
