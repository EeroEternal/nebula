# LLM Service Mesh (Unified Model Plane)

## Vision
Transform Nebula from a GPU cluster management tool into a **Universal Model Plane**. By decoupling the model serving interface from the underlying compute, Nebula can act as a unified gateway for both locally hosted (vLLM, SGLang, MLX) and externally hosted (DeepSeek, OpenAI, etc.) model services.

## Core Concepts
- **Proxy/Remote Endpoints**: Treat external API services as first-class citizens within the Nebula ecosystem.
- **Unified Interface**: All models, regardless of origin, are exposed via the standard Nebula Gateway (OpenAI-compatible).
- **Control Plane Consolidation**:
    - **Observability**: Centralized logging, auditing, and tracing (XTrace) for both local and remote models.
    - **Policy Enforcement**: Rate limiting, tenant-based routing, and authentication managed at the Nebula Gateway layer.
    - **Service Mesh for LLMs**: Nebula handles service discovery, load balancing, and failover across heterogeneous model sources.

## Proposed Architecture Evolution
1. **Endpoint Kind Expansion**: Introduce a new `EndpointKind::Proxy` alongside the existing `EndpointKind::NativeHttp`.
2. **Virtual Node Management**: Implement a "Virtual Node" controller in `nebula-node` that manages proxy endpoints without requiring physical GPU resources.
3. **Gateway Transparency**: Gateway handles traffic routing based on the model registry, with the router transparently dispatching to either local containers or remote API proxies.
4. **Metadata Integration**: Extend `model_spec.rs` and `model_deployment.rs` to support proxy configuration parameters (target URL, API keys, proxy-specific headers).

## Benefits
- **Vendor Agnostic**: Easily swap between local models and commercial APIs without changing application-level code.
- **Improved Security**: Centralize sensitive API keys within Nebula rather than distributing them across various client services.
- **Hybrid Serving**: Seamlessly combine local high-throughput models with remote high-capability models in the same production pipeline.
