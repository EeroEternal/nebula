# Nebula Visual Design System

This document solidifies the design language for **Nebula**, the Universal Model Plane and LLM Service Mesh. It follows the "Tech Utility" aesthetic, prioritizing high information density, technical precision, and architectural transparency.

---

## 1. Design Vision
Nebula's UI is a **Control Plane**, not a marketing site. It should feel like a specialized industrial instrument—precise, responsive, and deterministic.

### Core Principles
- **Technical Determinism:** Every pixel must have a functional purpose. Prefer clear grids over decorative whitespace.
- **Mesh Transparency:** Use layers and glassmorphism to visualize the hierarchical nature of the service mesh.
- **Atomic Capabilities:** Models and nodes are treated as modular "Skills" or blocks in a larger topology.

---

## 2. Foundation

### Color System (OKLch)
We use the OKLch color space for perceptual uniformity and vibrant dark-mode rendering.

| Layer | Value | Purpose |
| :--- | :--- | :--- |
| **Surface** | `oklch(18% 0.02 260)` | Deep space base background. |
| **Sub-surface** | `oklch(22% 0.03 260)` | Card and sidebar containers. |
| **Elevated** | `oklch(26% 0.04 260)` | Overlays and dialogs. |
| **Primary (Flow)** | `oklch(70% 0.18 190)` | Electric Cyan for active data and actions. |
| **Success** | `oklch(68% 0.22 150)` | Emerald for healthy/operational status. |
| **Warning** | `oklch(82% 0.16 80)` | Amber for latency or degraded states. |
| **Critical** | `oklch(60% 0.2 25)` | Ruby for failures and alerts. |

### Typography: The Protocol Stack
| Role | Font Family | Character |
| :--- | :--- | :--- |
| **Display** | `Geist Mono` | Mechanical, precise, high-contrast. |
| **Body** | `Inter (Variable)` | High legibility for status logs and data. |
| **Code/Data** | `Fira Code` | Monospaced for IDs, JSON, and endpoints. |

---

## 3. Geometry & Texture

### Surface Rules
- **Radius:** 
  - `4px (Sharp)` for small UI units (inputs, tags).
  - `12px (Soft)` for major containers (cards, dialogs).
- **Borders:** `1px` subtle stroke using `oklch(30% 0.05 260 / 0.5)`. Avoid drop shadows in favor of rim lights.
- **Glassmorphism:** Use `backdrop-blur(12px)` with `bg-card/40` for overlays to maintain spatial context.

### Interaction States
- **Rim Light:** Active or focused elements gain a `1px` inner highlight simulating hardware illumination.
- **Signal Pulse:** Active endpoints use a subtle opacity pulse (`animate-signal`) to represent real-time traffic flow.

---

## 4. Components & Patterns

### The Sidebar (Command Center)
Categorized into functional planes:
1.  **Workbench:** High-level overview and model access.
2.  **Infrastructure:** Bare-metal management (Nodes, Images).
3.  **Resources:** Asset discovery and historical ledgers.

### Topology Grids
Dashboard and Node views use a strict grid system to represent the cluster topology. Each GPU/Node is a self-contained module with visible performance metrics.

### Model Wizard
A 5-step sequential flow for complex model provisioning:
`Source` → `Identity` → `Hardware` → `Engine` → `Review`.

---

## 5. Implementation Standards
- **Framework:** React 19 + Vite 7.
- **Styling:** Tailwind CSS 4 (Utility-first with OKLch variables).
- **Data:** TanStack Query for state synchronization and polling.
- **I18n:** Full support for multi-language schemas (default: en/zh).
