//! Process runtime management

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use sysinfo::{Pid, System};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmd: Vec<String>,
    pub memory_mb: u64,
    pub cpu_percent: f32,
    pub start_time: u64,
    pub project: Option<String>,
    pub harness: Option<String>,
}

impl ProcessInfo {
    pub fn from_sysinfo(pid: Pid, name: String, sys: &System) -> Option<Self> {
        sys.process(pid).map(|p| {
            let status = p.process_status();
            ProcessInfo {
                pid: pid.as_u32(),
                name,
                cmd: p.cmd().iter().filter_map(|s| s.to_str().map(String::from)).collect(),
                memory_mb: p.memory() / 1024 / 1024,
                cpu_percent: p.cpu_usage(),
                start_time: p.start_time(),
                project: None,
                harness: None,
            }
        })
    }
}

#[derive(Debug)]
pub struct ManagedProcess {
    pub info: ProcessInfo,
    pub child: Option<Child>,
}

pub struct ProcessPool {
    processes: RwLock<HashMap<u32, ManagedProcess>>,
    system: RwLock<System>,
}

impl Default for ProcessPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessPool {
    pub fn new() -> Self {
        Self {
            processes: RwLock::new(HashMap::new()),
            system: RwLock::new(System::new_all()),
        }
    }

    /// Refresh system process information
    pub async fn refresh(&self) {
        let mut sys = self.system.write().await;
        sys.refresh_all();
    }

    /// Get all managed processes
    pub async fn list(&self) -> Vec<ProcessInfo> {
        let sys = self.system.read().await;
        let procs = self.processes.read().await;

        let mut result = Vec::new();
        for pid in procs.keys() {
            if let Some(info) = ProcessInfo::from_sysinfo(Pid::from_u32(*pid), procs.get(pid).unwrap().info.name.clone(), &sys) {
                result.push(info);
            }
        }
        result
    }

    /// Spawn a new process
    pub async fn spawn(
        &self,
        cmd: &str,
        args: &[String],
        cwd: Option<PathBuf>,
        project: Option<String>,
        harness: Option<String>,
    ) -> Result<ProcessInfo> {
        let mut command = Command::new(cmd);
        command.args(args);

        if let Some(ref path) = cwd {
            command.current_dir(path);
        }

        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(Stdio::null());

        let mut child = command.spawn()
            .context("Failed to spawn process")?;

        let pid = child.id().unwrap_or(0);

        // Refresh to get accurate info
        self.refresh().await;

        let sys = self.system.read().await;
        let info = ProcessInfo {
            pid,
            name: cmd.to_string(),
            cmd: vec![cmd.to_string()].into_iter().chain(args.iter().cloned()).collect(),
            memory_mb: 0,
            cpu_percent: 0.0,
            start_time: 0,
            project,
            harness,
        };

        let managed = ManagedProcess {
            info: info.clone(),
            child: Some(child),
        };

        let mut procs = self.processes.write().await;
        procs.insert(pid, managed);

        Ok(info)
    }

    /// Kill a process by PID
    pub async fn kill(&self, pid: u32) -> Result<()> {
        let mut procs = self.processes.write().await;
        if let Some(mut managed) = procs.remove(&pid) {
            if let Some(ref mut child) = managed.child {
                child.kill().await?;
            }
        }
        Ok(())
    }

    /// Kill all managed processes
    pub async fn kill_all(&self) -> Result<()> {
        let mut procs = self.processes.write().await;
        for (_, managed) in procs.drain() {
            if let Some(mut child) = managed.child {
                let _ = child.kill().await;
            }
        }
        Ok(())
    }

    /// Get process by PID
    pub async fn get(&self, pid: u32) -> Option<ProcessInfo> {
        let procs = self.processes.read().await;
        procs.get(&pid).map(|m| m.info.clone())
    }

    /// Check if process is still running
    pub async fn is_running(&self, pid: u32) -> bool {
        let procs = self.processes.read().await;
        if let Some(managed) = procs.get(&pid) {
            if let Some(ref child) = managed.child {
                return child.try_status().is_none();
            }
        }
        false
    }
}

/// Filter for specific process types
#[derive(Debug, Clone)]
pub enum ProcessFilter {
    All,
    ByName(String),
    ByProject(String),
    ByHarness(String),
    ByPattern(String),
}

impl ProcessPool {
    /// Find processes matching a filter
    pub async fn find(&self, filter: ProcessFilter) -> Vec<ProcessInfo> {
        self.refresh().await;
        let sys = self.system.read().await;
        let procs = self.processes.read().await;

        let mut result = Vec::new();

        for (pid, managed) in procs.iter() {
            let info = ProcessInfo::from_sysinfo(
                Pid::from_u32(*pid),
                managed.info.name.clone(),
                &sys,
            );

            if let Some(info) = info {
                let matches = match filter {
                    ProcessFilter::All => true,
                    ProcessFilter::ByName(ref name) => info.name.contains(name),
                    ProcessFilter::ByProject(ref proj) => info.project.as_ref() == Some(proj),
                    ProcessFilter::ByHarness(ref harness) => info.harness.as_ref() == Some(harness),
                    ProcessFilter::ByPattern(ref pat) => info.cmd.iter().any(|c| c.contains(pat)),
                };

                if matches {
                    result.push(info);
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_pool() {
        let pool = ProcessPool::new();

        // Spawn a simple process
        let info = pool.spawn("echo", &["hello".to_string()], None, None, None).await;
        assert!(info.is_ok());

        // List processes
        let list = pool.list().await;
        assert!(!list.is_empty());

        // Kill all
        pool.kill_all().await.unwrap();
    }
}
