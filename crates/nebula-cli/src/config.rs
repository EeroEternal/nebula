use nebula_common::ModelConfig;

pub fn build_config(
    tensor_parallel_size: Option<u32>,
    gpu_memory_utilization: Option<f32>,
    max_model_len: Option<u32>,
    required_vram_mb: Option<u64>,
    lora_modules: Option<Vec<String>>,
) -> Option<ModelConfig> {
    if tensor_parallel_size.is_none()
        && gpu_memory_utilization.is_none()
        && max_model_len.is_none()
        && required_vram_mb.is_none()
        && lora_modules.as_ref().is_none_or(|v| v.is_empty())
    {
        return None;
    }

    Some(ModelConfig {
        tensor_parallel_size,
        gpu_memory_utilization,
        max_model_len,
        required_vram_mb,
        lora_modules,
        ..Default::default()
    })
}
