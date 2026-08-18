//! `dirs` の型検査専用スタブ。実 dirs ではない (`README.md` 参照)。

use std::path::PathBuf;

pub fn home_dir() -> Option<PathBuf> {
    None
}

pub fn config_dir() -> Option<PathBuf> {
    None
}

pub fn data_dir() -> Option<PathBuf> {
    None
}
