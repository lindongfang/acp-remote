// Directory-contract checks create empty directories only: no adapter, Node or
// authentication files are opened. No process environment is mutated.
mod capacity_l6_identity_contract {
    use super::*;

    struct Area(std::path::PathBuf);

    impl Area {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = format!(
                "{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../l6-empty-directory-contract")
                .join(unique);
            std::fs::create_dir_all(&root).unwrap();
            Self(root.canonicalize().unwrap())
        }

        fn directory(&self, name: &str) -> std::path::PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        }

        fn config(&self) -> CapacityL6Config {
            CapacityL6Config {
                format: 1,
                run_root: self.directory("tester-owned").join("disposable-run"),
                launch_profile: self.0.join("unused-profile.json"),
                dedicated_home: self.directory("external-auth-home"),
                dedicated_codex_home: self.directory("external-codex-home"),
                allow_model_requests: false,
                default_limit: 50,
                idle_seconds: 1800,
                scenario_files: std::collections::BTreeMap::default(),
                web_control: None,
            }
        }

        fn defaults(&self) -> String {
            // This is a synthetic path string; no default identity is read.
            serde_json::to_string(&vec![self.0.join("unread-default")]).unwrap()
        }
    }

    impl Drop for Area {
        fn drop(&mut self) {
            let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../l6-empty-directory-contract")
                .canonicalize()
                .unwrap();
            assert!(self.0.starts_with(workspace) && self.0.file_name().is_some());
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn external_explicit_identity_accepts_without_tester_root_ownership() {
        let area = Area::new();
        let c = area.config();
        assert_eq!(
            capacity_l6_identity_guard(&c, &area.defaults()).unwrap(),
            c.dedicated_home.canonicalize().unwrap()
        );
        assert!(!c.dedicated_home.starts_with(c.run_root));
    }

    #[test]
    fn absent_empty_and_nonabsolute_original_default_facts_reject() {
        let area = Area::new();
        let c = area.config();
        for value in ["", "[]", "[\"\"]", "[\"relative-default\"]", "{}"] {
            assert!(capacity_l6_identity_guard(&c, value).is_err());
        }
    }

    #[test]
    fn default_identity_case_variant_rejects_without_reading_default_files() {
        let area = Area::new();
        let c = area.config();
        let defaults = serde_json::to_string(&vec![
            c.dedicated_home.to_string_lossy().to_ascii_uppercase(),
        ])
        .unwrap();
        assert_eq!(
            capacity_l6_identity_guard(&c, &defaults),
            Err("default_identity_directory_forbidden")
        );
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_unc_and_drive_defaults_match_without_network_access() {
        use std::path::Path;
        assert_eq!(
            capacity_l6_path_key(Path::new(r"\\?\UNC\Server\Share\Identity")).unwrap(),
            capacity_l6_path_key(Path::new(r"\\server\share\identity")).unwrap()
        );
        assert_eq!(
            capacity_l6_path_key(Path::new(r"\\?\D:\Identity")).unwrap(),
            capacity_l6_path_key(Path::new(r"d:\identity")).unwrap()
        );
    }

    #[test]
    fn disposable_and_identity_overlap_reject_in_both_directions() {
        let area = Area::new();
        let mut c = area.config();
        c.run_root = c.dedicated_home.join("run");
        assert_eq!(
            capacity_l6_identity_guard(&c, &area.defaults()),
            Err("identity_and_disposable_run_root_overlap")
        );
        c.run_root = area.directory("run");
        c.dedicated_home = area.directory("run/auth");
        assert_eq!(
            capacity_l6_identity_guard(&c, &area.defaults()),
            Err("identity_and_disposable_run_root_overlap")
        );
    }

    #[test]
    fn application_directory_requires_strict_child_and_existing_directory() {
        let area = Area::new();
        let c = area.config();
        let child = area.directory("external-auth-home/appdata");
        assert!(capacity_l6_strict_child(&child, &c.dedicated_home).is_ok());
        assert!(capacity_l6_strict_child(&c.dedicated_home, &c.dedicated_home).is_err());
        assert!(capacity_l6_strict_child(&c.dedicated_codex_home, &c.dedicated_home).is_err());
        assert!(capacity_l6_strict_child(&child.join("missing"), &c.dedicated_home).is_err());
        let escape = child.join("../../external-codex-home");
        assert!(capacity_l6_strict_child(&escape, &c.dedicated_home).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn reparse_ancestor_is_rejected_before_canonical_resolution() {
        let area = Area::new();
        let destination = area.directory("destination");
        let junction = area.0.join("junction");
        // A directory junction needs no symbolic-link privilege. Both exact
        // targets are owned empty directories created by this test.
        let output = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&destination)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "owned junction could not be created"
        );
        let result = capacity_l6_checked_directory(&junction);
        // Remove the link itself before Area's bounded directory cleanup.
        std::fs::remove_dir(&junction).unwrap();
        assert_eq!(result, Err("identity_directory_reparse_point_forbidden"));
    }

    #[test]
    fn profile_env_guard_accepts_only_explicit_safe_mode_and_no_browser_values() {
        use base64::Engine as _;
        let encode = |text: &str| {
            let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
            base64::engine::general_purpose::STANDARD.encode(bytes)
        };
        let entry = |k: &str, v: &str| {
            serde_json::json!({
                "name_utf16le_base64": encode(k),
                "value_utf16le_base64": encode(v),
            })
        };
        let area = Area::new();
        let c = area.config();
        let home = c.dedicated_home.canonicalize().unwrap();
        let appdata = area.directory("external-auth-home/appdata");
        let local = area.directory("external-auth-home/localappdata");
        let env_temp = area.directory("external-auth-home/temp");
        let short_tmp = area.directory("external-auth-home/tmp");
        let base = vec![
            entry("HOME", &c.dedicated_home.to_string_lossy()),
            entry("USERPROFILE", &c.dedicated_home.to_string_lossy()),
            entry("CODEX_HOME", &c.dedicated_codex_home.to_string_lossy()),
            entry("APPDATA", &appdata.to_string_lossy()),
            entry("LOCALAPPDATA", &local.to_string_lossy()),
            entry("TEMP", &env_temp.to_string_lossy()),
            entry("TMP", &short_tmp.to_string_lossy()),
        ];

        for mode in ["read-only", "workspace-write"] {
            let mut env = base.clone();
            env.push(entry("INITIAL_AGENT_MODE", mode));
            env.push(entry("NO_BROWSER", "1"));
            assert_eq!(capacity_l6_profile_env_guard(&env, &c, &home), Ok(()));
        }

        for bad_mode in ["agent", "READ-ONLY", ""] {
            let mut env = base.clone();
            env.push(entry("INITIAL_AGENT_MODE", bad_mode));
            assert_eq!(
                capacity_l6_profile_env_guard(&env, &c, &home),
                Err("profile_initial_agent_mode_invalid")
            );
        }

        for bad_nb in ["0", "true", ""] {
            let mut env = base.clone();
            env.push(entry("NO_BROWSER", bad_nb));
            assert_eq!(
                capacity_l6_profile_env_guard(&env, &c, &home),
                Err("profile_no_browser_invalid")
            );
        }

        let mut foreign = base;
        foreign.push(entry("OPENAI_API_KEY", "inert"));
        assert_eq!(
            capacity_l6_profile_env_guard(&foreign, &c, &home),
            Err("profile_environment_outside_explicit_safe_subset")
        );
    }
}
