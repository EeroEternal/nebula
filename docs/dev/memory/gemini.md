# Gemini Memory for Nebula Project

This document contains a log of important facts and project-specific knowledge accumulated by Gemini CLI during interactions with the Nebula codebase.

## Architectural Evolution
* **LLM Service Mesh (Universal Model Plane):** The Nebula project has evolved into an 'LLM Service Mesh' (Universal Model Plane). External model APIs are now treated as first-class endpoints alongside locally hosted models. Implementation involved adding a 'virtual' (Proxy) engine type, virtual node management, and unified observability for all traffic.
* **Virtual Engine Implementation:** Successfully implemented `virtual` (Proxy) engine support in `nebula-node`. Virtual engines act as an LLM Service Mesh, proxying requests to external APIs (like DeepSeek) using the `unigateway-sdk`, while behaving as native endpoints for the Gateway and Router. This unifies local GPU deployments and remote API integrations under a single, observable routing plane.
