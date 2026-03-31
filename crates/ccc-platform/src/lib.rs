pub mod detect;
pub mod vcs;

pub use detect::{get_platform, is_wsl, linux_distro_info, wsl_version, LinuxDistroInfo, Platform};
pub use vcs::{detect_vcs, Vcs};
