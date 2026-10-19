//! C-reference selection shared by conformance tests and xtask, never by the
//! simulator. Most fixtures use one golden; explicitly documented libm-sensitive
//! fixtures may have an independently captured platform variant. Missing data
//! is an error at the caller, not permission to fall back to another platform.

use std::path::{Path, PathBuf};

/// The C golden for `name` on the target running the verifier/capture tool.
/// Capture, drift checks and numerical verification must use the same path.
pub(crate) fn golden_path(root: &Path, name: &str) -> PathBuf {
    path_for(
        root,
        name,
        cfg!(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu"
        )),
    )
}

fn path_for(root: &Path, name: &str, linux_x86_64_gnu: bool) -> PathBuf {
    let mut directory = root.join("conformance/golden");
    // bjtsetup.c scales ISE=1e-3 by IS=1e-16, so cktsens.c's ISE
    // perturbation is 1e-25. Platform libm rounding is amplified by that
    // divisor: Linux C and Rust agree, but neither matches macOS C's residue.
    if name == "sens_bjt" && linux_x86_64_gnu {
        directory.push("linux-x86_64-gnu");
    }
    directory.join(format!("{name}.raw"))
}

#[cfg(test)]
mod tests {
    use super::{golden_path, path_for};
    use std::path::Path;

    #[test]
    fn only_the_documented_fixture_uses_a_platform_variant() {
        let root = Path::new("scratch");
        assert_eq!(
            path_for(root, "sens_bjt", true),
            root.join("conformance/golden/linux-x86_64-gnu/sens_bjt.raw")
        );
        assert_eq!(
            path_for(root, "sens_bjt", false),
            root.join("conformance/golden/sens_bjt.raw")
        );
        for name in ["rc_divider", "sens_ac_bjt", "sens_ac_mos_tran"] {
            assert_eq!(path_for(root, name, true), path_for(root, name, false));
        }
    }

    #[test]
    fn selection_does_not_depend_on_whether_the_file_exists() {
        let root = Path::new("not-an-existing-reference-directory");
        let selected = golden_path(root, "sens_bjt");
        assert!(!selected.exists());
        assert_eq!(
            selected,
            path_for(
                root,
                "sens_bjt",
                cfg!(all(
                    target_os = "linux",
                    target_arch = "x86_64",
                    target_env = "gnu"
                ))
            )
        );
    }
}
