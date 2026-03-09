/// Main entry point for pi-daemon CLI.
///
/// TODO(#11): Implement CLI commands — start, stop, status, chat
fn main() {
    println!("pi-daemon v{}", env!("CARGO_PKG_VERSION"));
    println!("CLI implementation pending - see issue #11");
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_version_is_valid_semver() {
        let version = env!("CARGO_PKG_VERSION");

        // Basic semver format check (x.y.z)
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3, "Version should have format x.y.z");

        // Each part should be a number
        for part in parts {
            part.parse::<u32>()
                .expect("Version parts should be numbers");
        }
    }

    #[test]
    fn test_version_output() {
        // Simple test to verify basic functionality
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}
