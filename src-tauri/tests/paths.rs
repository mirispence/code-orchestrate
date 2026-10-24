//! The shared path normaliser and comparison (spec §6.1).

use code_orchestrate_lib::paths::normalize;

fn norm(raw: &str) -> String {
    normalize(raw).expect("normalises").as_str().to_owned()
}

#[cfg(windows)]
mod windows {
    use super::*;

    #[test]
    fn uppercases_the_drive_letter_and_uses_backslashes() {
        assert_eq!(norm("e:/Programming/code"), r"E:\Programming\code");
    }

    #[test]
    fn resolves_dot_and_dot_dot() {
        assert_eq!(norm(r"C:\a\.\b\..\c"), r"C:\a\c");
        assert_eq!(norm(r"C:\a\..\..\b"), r"C:\b");
    }

    #[test]
    fn strips_trailing_separators() {
        assert_eq!(norm(r"C:\a\b\"), r"C:\a\b");
        assert_eq!(norm("C:/a/b//"), r"C:\a\b");
    }

    #[test]
    fn keeps_the_separator_on_a_drive_root() {
        assert_eq!(norm(r"C:\"), r"C:\");
        assert_eq!(norm("c:/"), r"C:\");
        assert_eq!(norm(r"C:\a\.."), r"C:\");
    }

    #[test]
    fn keeps_unc_shares() {
        assert_eq!(norm(r"\\server\share\dir\"), r"\\server\share\dir");
    }

    #[test]
    fn makes_relative_paths_absolute() {
        let cwd = std::env::current_dir().unwrap();
        let expected = normalize(cwd.join("child").to_str().unwrap()).unwrap();
        assert_eq!(norm("child"), expected.as_str());
        assert!(std::path::Path::new(&norm("child")).is_absolute());
    }

    #[test]
    fn displays_the_path_as_typed_apart_from_the_drive_letter() {
        assert_eq!(norm(r"c:\Users\Miri"), r"C:\Users\Miri");
    }

    #[test]
    fn compares_case_insensitively() {
        let a = normalize(r"E:\Code\Orchestrate").unwrap();
        let b = normalize(r"e:\code\orchestrate\").unwrap();
        assert!(a.same_as(&b));
        assert_eq!(a.comparison_key(), b.comparison_key());
    }

    #[test]
    fn ancestry_compares_whole_segments() {
        let code = normalize(r"E:\code").unwrap();
        assert!(!code.is_ancestor_of(&normalize(r"E:\code-orchestrate").unwrap()));
        assert!(code.is_ancestor_of(&normalize(r"E:\code\orchestrate").unwrap()));
        assert!(code.is_ancestor_of(&normalize(r"e:\CODE\orchestrate").unwrap()));
    }

    #[test]
    fn a_root_is_an_ancestor_of_its_children() {
        let root = normalize(r"C:\").unwrap();
        assert!(root.is_ancestor_of(&normalize(r"C:\a").unwrap()));
        assert!(!root.is_ancestor_of(&normalize(r"D:\a").unwrap()));
    }

    #[test]
    fn does_not_resolve_symlinks_or_junctions() {
        // A junction to another folder must keep its own path.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("link");
        std::fs::create_dir(&target).unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap();
        assert!(status.status.success(), "mklink /J failed");

        let normalised = norm(link.to_str().unwrap());
        assert!(normalised.ends_with(r"\link"), "{normalised}");
    }
}

#[cfg(not(windows))]
mod unix {
    use super::*;

    #[test]
    fn resolves_dot_and_dot_dot_and_trailing_slashes() {
        assert_eq!(norm("/a/./b/../c/"), "/a/c");
    }

    #[test]
    fn keeps_the_root() {
        assert_eq!(norm("/"), "/");
        assert_eq!(norm("/a/.."), "/");
    }

    #[test]
    fn compares_case_sensitively() {
        assert!(!normalize("/A").unwrap().same_as(&normalize("/a").unwrap()));
    }

    #[test]
    fn ancestry_compares_whole_segments() {
        let code = normalize("/code").unwrap();
        assert!(!code.is_ancestor_of(&normalize("/code-orchestrate").unwrap()));
        assert!(code.is_ancestor_of(&normalize("/code/x").unwrap()));
        assert!(normalize("/").unwrap().is_ancestor_of(&code));
    }
}

#[test]
fn a_path_is_not_its_own_ancestor() {
    let p = normalize(if cfg!(windows) { r"C:\a" } else { "/a" }).unwrap();
    assert!(!p.is_ancestor_of(&p));
}

#[test]
fn normalising_is_idempotent() {
    let once = normalize(if cfg!(windows) { r"c:\a\..\b\" } else { "/a/../b/" }).unwrap();
    let twice = normalize(once.as_str()).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn keeps_whitespace_that_is_part_of_a_folder_name() {
    let raw = if cfg!(windows) { "C:\\a\\b\u{a0}" } else { "/a/b\u{a0}" };
    assert!(norm(raw).ends_with("b\u{a0}"), "{:?}", norm(raw));
}

#[test]
fn rejects_an_empty_path() {
    assert!(normalize("").is_err());
    assert!(normalize("   ").is_err());
}
