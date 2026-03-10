//! Pi-specific configuration — re-exports from `pi-daemon-types`.

/// Re-export `PiManagerConfig` as `PiConfig` for convenience within this crate.
pub type PiConfig = pi_daemon_types::config::PiManagerConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pi_config_default() {
        let config = PiConfig::default();
        assert_eq!(config.binary_path, "");
        assert_eq!(config.min_version, "0.56.0");
        assert!(config.auto_install);
        assert!(config.auto_start);
        assert_eq!(config.pool_size, 1);
        assert_eq!(config.working_directory, "~");
        assert_eq!(config.managed_extensions, vec!["pi-daemon-bridge"]);
        assert!(config.extra_flags.is_empty());
    }

    #[test]
    fn test_pi_config_serialization_roundtrip() {
        let config = PiConfig {
            binary_path: "/usr/local/bin/pi".to_string(),
            min_version: "0.57.0".to_string(),
            auto_install: false,
            auto_start: false,
            pool_size: 2,
            working_directory: "/home/user".to_string(),
            managed_extensions: vec!["pi-daemon-bridge".to_string(), "custom-ext".to_string()],
            extra_flags: vec!["--verbose".to_string()],
        };

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: PiConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.binary_path, "/usr/local/bin/pi");
        assert_eq!(parsed.min_version, "0.57.0");
        assert!(!parsed.auto_install);
        assert!(!parsed.auto_start);
        assert_eq!(parsed.pool_size, 2);
        assert_eq!(parsed.managed_extensions.len(), 2);
        assert_eq!(parsed.extra_flags, vec!["--verbose"]);
    }

    #[test]
    fn test_pi_config_deserialize_with_missing_fields() {
        let toml_str = r#"
            binary_path = "/custom/pi"
            auto_start = false
        "#;

        let config: PiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.binary_path, "/custom/pi");
        assert!(!config.auto_start);
        assert_eq!(config.min_version, "0.56.0");
        assert!(config.auto_install);
        assert_eq!(config.pool_size, 1);
    }
}
