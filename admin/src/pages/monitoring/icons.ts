/**
 * 监控页统一图标映射（lucide-react）。
 * 页面按语义引用，避免散落重复 import。
 */
import {
  LayoutDashboard,
  Server,
  Cpu,
  CircuitBoard,
  Gauge,
  Crown,
  RefreshCw,
  Download,
  ExternalLink,
  AlertTriangle,
  MemoryStick,
  Activity,
  Hash,
  Layers,
  Clock,
  Circle,
  CircleOff,
  Inbox,
  Shield,
  type LucideIcon,
} from 'lucide-react';

export type MonitorIconName =
  | 'overview'
  | 'compute'
  | 'nodes'
  | 'hardware'
  | 'accelerator'
  | 'npu'
  | 'gauge'
  | 'haPrimary'
  | 'refresh'
  | 'download'
  | 'externalLink'
  | 'alert'
  | 'vram'
  | 'activity'
  | 'hash'
  | 'layers'
  | 'clock'
  | 'online'
  | 'offline'
  | 'empty'
  | 'shield';

export const MonitorIcons: Record<MonitorIconName, LucideIcon> = {
  overview: LayoutDashboard,
  compute: Cpu,
  nodes: Server,
  hardware: Activity,
  accelerator: CircuitBoard,
  npu: CircuitBoard,
  gauge: Gauge,
  haPrimary: Crown,
  refresh: RefreshCw,
  download: Download,
  externalLink: ExternalLink,
  alert: AlertTriangle,
  vram: MemoryStick,
  activity: Activity,
  hash: Hash,
  layers: Layers,
  clock: Clock,
  online: Circle,
  offline: CircleOff,
  empty: Inbox,
  shield: Shield,
};

export function monitorIcon(name: MonitorIconName): LucideIcon {
  return MonitorIcons[name];
}
