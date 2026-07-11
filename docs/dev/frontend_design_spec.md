# Nebula Frontend Design Specification 2.0

## 1. Core Design Philosophy
- **Technical Determinism:** Logic-driven layouts, clear grids, and sharp edges.
- **Mesh Transparency:** Layered surfaces and glassmorphism to visualize the "Service Mesh" topology.
- **Atomic Capabilities:** Models and nodes treated as composable "Skills" (Reasoning, Coding, etc.).

## 2. Color System (OKLch)
We use the OKLch color space for perceptual uniformity.

### Foundation
- **Surface (Base):** `oklch(18% 0.02 260)` - Deep space blue.
- **Sub-surface (Card):** `oklch(22% 0.03 260)` - Secondary layer.
- **Elevated (Dialog):** `oklch(26% 0.04 260)` - Top layer.

### Signal (Traffic & Status)
- **Primary (Flow):** `oklch(70% 0.18 190)` - Electric Cyan (Active data flow).
- **Virtual (Proxy):** `oklch(75% 0.12 280)` - Amethyst (External/Virtual nodes).
- **Health (Ready):** `oklch(68% 0.22 150)` - Emerald (Normal operation).
- **Warning (Latency):** `oklch(82% 0.16 80)` - Amber (High latency/Issues).

## 3. Typography: The Protocol Stack
- **Display/Headings:** `Geist` or `JetBrains Mono` (Bold).
- **Body:** `Inter` (Variable).
- **Technical Data:** `Fira Code`.

## 4. Geometry & Texture
- **Border Radius:**
  - `4px (Sharp)`: Small components, tags, inputs.
  - `12px (Soft)`: Main containers, dialogs.
- **Borders:** `1px` subtle stroke using `oklch(30% 0.05 260 / 0.5)`. No heavy shadows.
- **Interactions:** "Rim light" effect (1px bright border) for active/focused states.
- **Backgrounds:** `backdrop-blur(12px)` for overlays and sidebars.

## 5. Component Patterns
- **Skills Matrix:** Visualizing model capabilities as a grid of active nodes.
- **Pulse:** Subtle cyan shadow animation for active endpoints.
- **Ticker Bar:** Real-time metrics bar (RPS, Latency) inspired by terminal dashboards.

## 6. Layout: Topology Framework
- **Control Plane:** Infrastructure management (Nodes, Images).
- **Model Plane:** Registry and deployments (Catalog, Library).
- **Traffic Plane:** Observability and routing (Gateway, Audit).
