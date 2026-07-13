use tokio::process::Command;

use nebula_common::GpuStatus;

pub async fn read_gpu_statuses() -> Vec<GpuStatus> {
    let output = Command::new("nvidia-smi")
        .arg("--query-gpu=name,driver_version,memory.total,memory.used,temperature.gpu,utilization.gpu")
        .arg("--format=csv,noheader,nounits")
        .output()
        .await;

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let cuda_version = read_cuda_version().await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for (idx, line) in stdout.lines().enumerate() {
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        // name, driver, mem.total, mem.used, temp, util  — name may contain commas? rare; keep simple
        if parts.len() < 4 {
            continue;
        }
        let name = Some(parts[0].to_string()).filter(|s| !s.is_empty());
        let driver_version = Some(parts[1].to_string()).filter(|s| !s.is_empty());
        let total = parts[2].parse::<u64>().unwrap_or(0);
        let used = parts[3].parse::<u64>().unwrap_or(0);
        let temperature = parts.get(4).and_then(|s| s.parse::<u32>().ok());
        let utilization = parts.get(5).and_then(|s| s.parse::<u32>().ok());
        out.push(GpuStatus {
            index: idx as u32,
            memory_total_mb: total,
            memory_used_mb: used,
            temperature_c: temperature,
            utilization_gpu: utilization,
            name,
            driver_version,
            cuda_version: cuda_version.clone(),
        });
    }
    out
}

async fn read_cuda_version() -> Option<String> {
    let output = Command::new("nvidia-smi")
        .arg("--query-gpu=driver_version")
        .arg("--format=csv,noheader")
        .output()
        .await
        .ok()?;
    // Prefer CUDA Version line from plain nvidia-smi header.
    let header = Command::new("nvidia-smi").output().await.ok()?;
    if !header.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&header.stdout);
    for line in text.lines() {
        if let Some(idx) = line.find("CUDA Version:") {
            let rest = line[idx + "CUDA Version:".len()..].trim();
            let ver = rest.split_whitespace().next()?.to_string();
            if !ver.is_empty() {
                return Some(ver);
            }
        }
    }
    let _ = output;
    None
}
