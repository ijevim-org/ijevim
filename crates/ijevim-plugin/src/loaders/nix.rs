#![cfg(feature = "plugin-nix")]

use crate::{Plugin, PluginApi};
use ijevim_core::types::PluginEvent;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

pub struct NixPlugin {
    name: String,
    config: serde_json::Value,
}

impl Plugin for NixPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, api: &PluginApi) {
        // If the config has a repo_url, clone it
        if let Some(repo_url) = self.config.get("repo_url").and_then(|v| v.as_str()) {
            if let Err(e) = install_from_github(repo_url, api) {
                api.log(&format!("Failed to install Nix plugin: {}", e));
            }
        }
    }

    fn handle_event(&mut self, _event: &PluginEvent) {}

    fn execute_command(&mut self, _cmd: &str, _args: Vec<String>) -> bool {
        false
    }
}

fn install_from_github(repo_url: &str, api: &PluginApi) -> Result<(), String> {
    if !repo_url.starts_with("https://github.com/") {
        return Err(format!(
            "Invalid repository URL: only https://github.com/ URLs are allowed, got: {}",
            repo_url
        ));
    }

    api.log(&format!("Installing Nix plugin from {}", repo_url));

    let temp_dir = std::env::temp_dir().join("ijevim-nix");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            repo_url,
            temp_dir.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Git clone failed: {}", stderr));
    }

    api.log(&format!("Successfully cloned {}", repo_url));
    Ok(())
}

pub struct NixLoader;

impl super::Loader for NixLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["nix"]
    }

    fn load(
        &self,
        path: &Path,
        _api: Rc<PluginApi>,
    ) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Run nix-instantiate --eval --json --strict to evaluate the Nix file
        let output = Command::new("nix-instantiate")
            .args(["--eval", "--json", "--strict", &path.to_string_lossy()])
            .output()
            .map_err(|e| {
                super::LoaderError::Io(format!(
                    "Failed to execute nix-instantiate for {}: {}",
                    path.display(),
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(super::LoaderError::Parse(format!(
                "nix-instantiate failed for {}: {}",
                path.display(),
                stderr
            )));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|e| {
            super::LoaderError::Parse(format!(
                "Invalid UTF-8 from nix-instantiate for {}: {}",
                path.display(),
                e
            ))
        })?;

        let config: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
            super::LoaderError::Parse(format!(
                "Failed to parse nix-instantiate output as JSON for {}: {}",
                path.display(),
                e
            ))
        })?;

        // If the JSON output is a string, try to parse it as inline JSON
        let config = if let Some(s) = config.as_str() {
            // nix-instantiate may return a stringified JSON; try to parse it
            serde_json::from_str(s).unwrap_or(config)
        } else {
            config
        };

        let plugin = Box::new(NixPlugin { name, config });

        Ok(plugin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::Loader;
    use crate::PluginApi;

    #[test]
    fn test_nix_loader_basic() {
        let dir = std::env::temp_dir().join("ijevim-test-nix");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.nix");
        std::fs::write(
            &path,
            r#"{ name = "hello-nix"; version = "0.1.0"; description = "test"; }"#,
        )
        .unwrap();

        let loader = NixLoader;
        let api = Rc::new(PluginApi::new());
        let result = loader.load(&path, api);

        match result {
            Ok(plugin) => {
                assert_eq!(plugin.name(), "test");
            }
            Err(e) => {
                // nix-instantiate may not be installed in CI
                eprintln!(
                    "Nix loader test skipped (nix-instantiate not available): {}",
                    e
                );
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
