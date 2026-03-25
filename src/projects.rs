//! Project registry management

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub harnesses: Vec<String>,
}

impl Project {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            harnesses: vec![],
        }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn is_git_repo(&self) -> bool {
        self.path.join(".git").exists()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRegistry {
    pub projects: HashMap<String, Project>,
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectRegistry {
    pub fn new() -> Self {
        Self {
            projects: HashMap::new(),
        }
    }

    pub fn add(&mut self, project: Project) -> Result<()> {
        if !project.exists() {
            anyhow::bail!("Project path does not exist: {:?}", project.path);
        }
        self.projects.insert(project.name.clone(), project);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Option<Project> {
        self.projects.remove(name)
    }

    pub fn get(&self, name: &str) -> Option<&Project> {
        self.projects.get(name)
    }

    pub fn list(&self) -> Vec<&Project> {
        self.projects.values().collect()
    }

    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .context("Failed to read registry file")?;
            serde_json::from_str(&content).context("Failed to parse registry")
        } else {
            Ok(Self::new())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize registry")?;
        std::fs::write(path, content)
            .context("Failed to write registry file")?;
        Ok(())
    }

    /// Discover projects in a directory
    pub fn discover(base_path: &Path) -> Vec<Project> {
        let mut projects = Vec::new();

        if let Ok(entries) = std::fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".git").exists() {
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    projects.push(Project::new(name, &path));
                }
            }
        }

        projects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_registry() {
        let mut registry = ProjectRegistry::new();
        let project = Project::new("test", "/tmp/test");
        registry.add(project).unwrap();

        assert!(registry.get("test").is_some());
        assert!(registry.get("nonexistent").is_none());

        registry.remove("test");
        assert!(registry.get("test").is_none());
    }
}
