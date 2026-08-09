use tokio::process::Command;

/// Docker container runtime state for engine probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerContainerState {
    pub running: bool,
    pub oom_killed: bool,
    pub exit_code: Option<i32>,
    pub status: String,
}

/// Inspect a Docker container by name (Nebula-managed engines).
pub async fn inspect_docker_container(name: &str) -> Option<DockerContainerState> {
    let output = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{.State.Running}}|{{.State.OOMKilled}}|{{.State.ExitCode}}|{{.State.Status}}",
            name,
        ])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().splitn(4, '|').collect();
    if parts.len() < 4 {
        return None;
    }

    Some(DockerContainerState {
        running: parts[0] == "true",
        oom_killed: parts[1] == "true",
        exit_code: parts[2].parse().ok(),
        status: parts[3].to_string(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_inspect_template_fields() {
        let line = "false|true|137|exited";
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        assert_eq!(parts[0], "false");
        assert_eq!(parts[1], "true");
        assert_eq!(parts[2], "137");
        assert_eq!(parts[3], "exited");
    }
}
