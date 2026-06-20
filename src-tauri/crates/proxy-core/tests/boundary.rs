use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_DEPENDENCIES: &[&str] = &["tauri", "rusqlite", "sqlx"];

const FORBIDDEN_SOURCE_MARKERS: &[&str] = &[
    "crate::database",
    "crate :: database",
    "crate::settings",
    "crate :: settings",
    "crate::services",
    "crate :: services",
    "tauri::",
    "rusqlite::",
    "sqlx::",
];

#[test]
fn proxy_core_has_no_host_dependencies_or_imports() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_no_forbidden_dependencies(&manifest_dir.join("Cargo.toml"));
    assert_no_forbidden_source_markers(&manifest_dir.join("src"));
}

fn assert_no_forbidden_dependencies(manifest_path: &Path) {
    let manifest = fs::read_to_string(manifest_path).expect("read proxy-core Cargo.toml");
    let mut violations = Vec::new();

    for dependency in FORBIDDEN_DEPENDENCIES {
        if toml_declares_dependency(&manifest, dependency) {
            violations.push(format!(
                "Cargo.toml declares forbidden dependency `{dependency}`"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "proxy-core must remain host-neutral:\n{}",
        violations.join("\n")
    );
}

fn toml_declares_dependency(manifest: &str, dependency: &str) -> bool {
    let dependency_table = format!("[dependencies.{dependency}]");
    let dev_dependency_table = format!("[dev-dependencies.{dependency}]");
    let build_dependency_table = format!("[build-dependencies.{dependency}]");

    manifest.lines().any(|line| {
        let line = line.split('#').next().unwrap_or_default().trim();
        line == dependency_table
            || line == dev_dependency_table
            || line == build_dependency_table
            || line
                .strip_prefix(dependency)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

fn assert_no_forbidden_source_markers(src_dir: &Path) {
    let mut rust_files = Vec::new();
    collect_rust_files(src_dir, &mut rust_files);

    let mut violations = Vec::new();
    for path in rust_files {
        let source = fs::read_to_string(&path).expect("read proxy-core source file");
        for (line_index, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for marker in FORBIDDEN_SOURCE_MARKERS {
                if code.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains forbidden host marker `{}`",
                        path.display(),
                        line_index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "proxy-core must not import host-only modules or SDKs:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read proxy-core source directory") {
        let entry = entry.expect("read proxy-core source entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
