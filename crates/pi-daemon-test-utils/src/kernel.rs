use std::path::PathBuf;
use tempfile::TempDir;

/// Test kernel configuration with temporary directory.
///
/// This provides an isolated environment for testing kernel components
/// without affecting the real configuration or filesystem.
pub struct TestKernel {
    pub data_dir: PathBuf,
    pub _temp_dir: TempDir, // Keep alive to prevent cleanup
}

impl TestKernel {
    /// Create a new test kernel with temporary data directory.
    ///
    /// The temporary directory will be automatically cleaned up when
    /// the TestKernel instance is dropped.
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temporary directory");
        let data_dir = temp_dir.path().to_path_buf();

        Self {
            data_dir,
            _temp_dir: temp_dir,
        }
    }
}

impl Default for TestKernel {
    fn default() -> Self {
        Self::new()
    }
}
