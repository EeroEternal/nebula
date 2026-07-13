import { useMemo } from "react";
import { BarChart, Bar, XAxis, YAxis, ResponsiveContainer, CartesianGrid, Tooltip } from "recharts";
import { Activity, Zap, TrendingUp, Gauge, Timer, AlertTriangle, ShieldCheck, Server, Globe, BarChart3 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useI18n } from "@/lib/i18n";
import { useClusterOverview } from "@/hooks/useClusterOverview";
import { useEngineStats } from "@/hooks/useEngineStats";
import { useMetricsRaw } from "@/hooks/useMetricsRaw";
import { cn } from "@/lib/utils";

interface AccessMetrics {
  requests_total: number;
  requests_inflight: number;
  responses_2xx: number;
  responses_4xx: number;
  responses_5xx: number;
  /** Present only when Gateway /metrics is available; otherwise null. */
  auth: {
    missing: number;
    invalid: number;
    forbidden: number;
    rate_limited: number;
  } | null;
  data_source: "router" | "gateway" | "mixed";
}

interface RouterModelMetrics {
  model_uid: string;
  route_2xx: number;
  route_4xx: number;
  route_5xx: number;
  latency_count: number;
  latency_sum: number;
  ttft_count: number;
  ttft_sum: number;
}

/** Parse Router (and optional Gateway) Prometheus text. Prefer Router as the control-plane scrape. */
function parseAccessMetrics(raw: string = ""): AccessMetrics {
  const m: AccessMetrics = {
    requests_total: 0, requests_inflight: 0,
    responses_2xx: 0, responses_4xx: 0, responses_5xx: 0,
    auth: null,
    data_source: "router",
  };
  let sawGateway = false;
  let sawRouter = false;
  let authMissing = 0, authInvalid = 0, authForbidden = 0, authLimited = 0;
  let sawAuth = false;

  for (const line of raw.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const parts = trimmed.split(/\s+/);
    if (parts.length < 2) continue;
    const [key, val] = parts;
    const n = parseInt(val, 10);
    if (isNaN(n)) continue;

    if (key === "nebula_router_requests_total") { m.requests_total = n; sawRouter = true; }
    else if (key === "nebula_router_requests_inflight") { m.requests_inflight = n; sawRouter = true; }
    else if (key === "nebula_router_responses_2xx") { m.responses_2xx = n; sawRouter = true; }
    else if (key === "nebula_router_responses_4xx") { m.responses_4xx = n; sawRouter = true; }
    else if (key === "nebula_router_responses_5xx") { m.responses_5xx = n; sawRouter = true; }
    else if (key === "nebula_gateway_requests_total") { if (!sawRouter) m.requests_total = n; sawGateway = true; }
    else if (key === "nebula_gateway_requests_inflight") { if (!sawRouter) m.requests_inflight = n; sawGateway = true; }
    else if (key === "nebula_gateway_responses_2xx") { if (!sawRouter) m.responses_2xx = n; sawGateway = true; }
    else if (key === "nebula_gateway_responses_4xx") { if (!sawRouter) m.responses_4xx = n; sawGateway = true; }
    else if (key === "nebula_gateway_responses_5xx") { if (!sawRouter) m.responses_5xx = n; sawGateway = true; }
    else if (key === "nebula_gateway_auth_missing") { authMissing = n; sawAuth = true; }
    else if (key === "nebula_gateway_auth_invalid") { authInvalid = n; sawAuth = true; }
    else if (key === "nebula_gateway_auth_forbidden") { authForbidden = n; sawAuth = true; }
    else if (key === "nebula_gateway_auth_rate_limited") { authLimited = n; sawAuth = true; }
  }

  if (sawAuth) {
    m.auth = {
      missing: authMissing,
      invalid: authInvalid,
      forbidden: authForbidden,
      rate_limited: authLimited,
    };
  }
  m.data_source = sawRouter && sawGateway ? "mixed" : sawGateway && !sawRouter ? "gateway" : "router";
  return m;
}

function extractLabel(line: string, label: string): string | null {
  const re = new RegExp(`${label}="([^"]+)"`);
  const m = re.exec(line);
  return m ? m[1] : null;
}

function parseRouterModelMetrics(raw: string = ""): RouterModelMetrics[] {
  const map = new Map<string, RouterModelMetrics>();
  const ensure = (uid: string) => {
    if (!map.has(uid)) map.set(uid, { model_uid: uid, route_2xx: 0, route_4xx: 0, route_5xx: 0, latency_count: 0, latency_sum: 0, ttft_count: 0, ttft_sum: 0 });
    return map.get(uid)!;
  };

  for (const line of raw.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const parts = trimmed.split(/\s+/);
    if (parts.length < 2) continue;
    const [key, val] = parts;
    const n = parseFloat(val);
    if (isNaN(n)) continue;
    const uid = extractLabel(key, "model_uid");
    if (!uid) continue;

    if (key.startsWith("nebula_route_total")) {
      const status = extractLabel(key, "status");
      const m = ensure(uid);
      if (status === "2xx") m.route_2xx = n;
      else if (status === "4xx") m.route_4xx = n;
      else if (status === "5xx") m.route_5xx = n;
    } else if (key.startsWith("nebula_route_latency_seconds_count")) {
      ensure(uid).latency_count = n;
    } else if (key.startsWith("nebula_route_latency_seconds_sum")) {
      ensure(uid).latency_sum = n;
    } else if (key.startsWith("nebula_route_ttft_seconds_count")) {
      ensure(uid).ttft_count = n;
    } else if (key.startsWith("nebula_route_ttft_seconds_sum")) {
      ensure(uid).ttft_sum = n;
    }
  }
  return Array.from(map.values());
}

export function InferenceView() {
  const { t } = useI18n();
  const { data: overview } = useClusterOverview();
  const { data: engineStats = [] } = useEngineStats();
  const { data: metricsRaw = "" } = useMetricsRaw();

  const access = useMemo(() => parseAccessMetrics(metricsRaw), [metricsRaw]);
  const routerModels = useMemo(() => parseRouterModelMetrics(metricsRaw), [metricsRaw]);

  const successRate = access.requests_total > 0
    ? ((access.responses_2xx / access.requests_total) * 100).toFixed(1)
    : "—";

  const overloadedModels = useMemo(() => {
    const byModel = new Map<string, any[]>();
    for (const s of engineStats) {
      if (!byModel.has(s.model_uid)) byModel.set(s.model_uid, []);
      byModel.get(s.model_uid)!.push(s);
    }
    const result: string[] = [];
    for (const [uid, stats] of byModel) {
      const allOverloaded = stats.every(s => {
        return typeof s.kv_cache_usage === 'number' && s.kv_cache_usage > 0.95;
      });
      if (allOverloaded && stats.length > 0) result.push(uid);
    }
    return result;
  }, [engineStats]);

  const fmtMs = (seconds: number) => {
    if (seconds < 0.001) return "<1ms";
    if (seconds < 1) return `${(seconds * 1000).toFixed(0)}ms`;
    return `${seconds.toFixed(2)}s`;
  };

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
        <div>
          <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('inference.title')}</h2>
          <p className="text-muted-foreground mt-2 flex items-center gap-2">
            <Activity className="h-4 w-4 text-primary animate-signal" />
            {t('inference.subtitle')}
          </p>
        </div>
      </div>

      {overloadedModels.length > 0 && (
        <div className="bg-destructive/5 border border-destructive/30 rounded-xl p-5 flex items-center gap-4 rim-light">
          <AlertTriangle className="h-6 w-6 text-destructive animate-pulse" />
          <div>
            <p className="text-xs font-bold text-destructive uppercase tracking-widest">{t('inference.overloadDetected')}</p>
            <p className="text-[10px] text-destructive/70 uppercase mt-1 tracking-wider">
              {t('inference.overloadDesc', { models: overloadedModels.join(', ') })}
            </p>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        <MetricCard label={t('inference.totalRequests')} value={access.requests_total.toLocaleString()} icon={Globe} />
        <MetricCard label={t('inference.inFlight')} value={access.requests_inflight.toLocaleString()} icon={Gauge} />
        <MetricCard label={t('inference.activeEndpoints')} value={String(overview?.endpoints.length || 0)} icon={Zap} />
        <MetricCard label={t('inference.successRate')} value={successRate === "—" ? "—" : `${successRate}%`} icon={TrendingUp} color={Number(successRate) < 95 ? "text-warning" : "text-success"} />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl">
          <div className="flex items-center gap-2 mb-8">
            <BarChart3 className="h-4 w-4 text-muted-foreground" />
            <h3 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">{t('inference.responseStatus')}</h3>
          </div>
          <div className="h-64">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={[
                { name: "2xx", count: access.responses_2xx, fill: "oklch(68% 0.22 150)" },
                { name: "4xx", count: access.responses_4xx, fill: "oklch(82% 0.16 80)" },
                { name: "5xx", count: access.responses_5xx, fill: "oklch(60% 0.2 25)" },
              ]}>
                <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="oklch(30% 0.05 260 / 0.2)" />
                <XAxis dataKey="name" axisLine={false} tickLine={false} tick={{ fontSize: 10, fill: "oklch(75% 0.02 260)" }} />
                <YAxis axisLine={false} tickLine={false} tick={{ fontSize: 10, fill: "oklch(75% 0.02 260)" }} />
                <Tooltip cursor={{ fill: "oklch(100% 0 0 / 0.05)" }} contentStyle={{ backgroundColor: "oklch(22% 0.03 260)", border: "1px solid oklch(30% 0.05 260 / 0.5)", borderRadius: "8px" }} />
                <Bar dataKey="count" radius={[4, 4, 0, 0]} barSize={40} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        </div>

        <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl">
          <div className="flex items-center gap-2 mb-8">
            <ShieldCheck className="h-4 w-4 text-muted-foreground" />
            <h3 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">{t('inference.authEvents')}</h3>
          </div>
          {access.auth ? (
          <div className="h-64">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={[
                { name: "MISSING", count: access.auth.missing },
                { name: "INVALID", count: access.auth.invalid },
                { name: "FORBIDDEN", count: access.auth.forbidden },
                { name: "LIMITED", count: access.auth.rate_limited },
              ]}>
                <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="oklch(30% 0.05 260 / 0.2)" />
                <XAxis dataKey="name" axisLine={false} tickLine={false} tick={{ fontSize: 9, fill: "oklch(75% 0.02 260)" }} />
                <YAxis axisLine={false} tickLine={false} tick={{ fontSize: 10, fill: "oklch(75% 0.02 260)" }} />
                <Tooltip cursor={{ fill: "oklch(100% 0 0 / 0.05)" }} contentStyle={{ backgroundColor: "oklch(22% 0.03 260)", border: "1px solid oklch(30% 0.05 260 / 0.5)", borderRadius: "8px" }} />
                <Bar dataKey="count" fill="oklch(75% 0.12 280)" radius={[4, 4, 0, 0]} barSize={40} />
              </BarChart>
            </ResponsiveContainer>
          </div>
          ) : (
            <div className="h-64 flex items-center justify-center text-xs font-mono text-muted-foreground">
              n/a — Gateway auth metrics not in this scrape ({access.data_source})
            </div>
          )}
        </div>
      </div>

      {routerModels.length > 0 && (
        <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
            <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
                <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">{t('inference.perModelRouting')}</h3>
            </div>
            <Table>
                <TableHeader className="bg-black/20">
                    <TableRow className="border-border/50 hover:bg-transparent">
                        <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">Model ID</TableHead>
                        <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Volume</TableHead>
                        <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Response Profile</TableHead>
                        <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Avg Latency</TableHead>
                        <TableHead className="text-[10px] uppercase font-bold text-muted-foreground text-right pr-6">Avg TTFT</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {routerModels.map((rm) => (
                        <TableRow key={rm.model_uid} className="border-border/40 hover:bg-white/5 transition-colors group">
                            <TableCell className="px-6 py-4 font-mono text-xs font-bold text-foreground group-hover:text-primary transition-colors">{rm.model_uid}</TableCell>
                            <TableCell className="font-mono text-xs font-bold">{(rm.route_2xx + rm.route_4xx + rm.route_5xx).toLocaleString()}</TableCell>
                            <TableCell>
                                <div className="flex items-center gap-3">
                                    <div className="flex flex-col">
                                        <span className="text-[10px] font-bold text-success">2XX: {rm.route_2xx}</span>
                                        <span className="text-[10px] font-bold text-warning">4XX: {rm.route_4xx}</span>
                                    </div>
                                    <div className="h-6 w-px bg-border/50" />
                                    <span className="text-[10px] font-bold text-destructive">5XX: {rm.route_5xx}</span>
                                </div>
                            </TableCell>
                            <TableCell>
                                <div className="flex items-center gap-2">
                                    <Timer className="h-3 w-3 text-muted-foreground" />
                                    <span className="text-xs font-mono font-bold">{rm.latency_count > 0 ? fmtMs(rm.latency_sum / rm.latency_count) : "—"}</span>
                                </div>
                            </TableCell>
                            <TableCell className="text-right pr-6">
                                <div className="flex items-center justify-end gap-2">
                                    <Zap className="h-3 w-3 text-primary" />
                                    <span className="text-xs font-mono font-bold">{rm.ttft_count > 0 ? fmtMs(rm.ttft_sum / rm.ttft_count) : "—"}</span>
                                </div>
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
      )}

      {engineStats.length > 0 && (
        <div className="space-y-4">
             <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground px-2">{t('inference.engineStats')}</h3>
             <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
                {engineStats.map((s) => {
                  const hasKv = typeof s.kv_cache_usage === 'number';
                  const kvPct = hasKv ? Math.round(s.kv_cache_usage! * 100) : null;
                  const isOverloaded = kvPct != null && kvPct > 95;
                  return (
                    <div key={`${s.model_uid}-${s.replica_id}`} className="bg-card/40 backdrop-blur-xl border border-border p-5 rounded-xl rim-light space-y-5">
                      <div className="flex items-center justify-between">
                        <span className="font-mono text-xs font-bold text-foreground">{s.model_uid}</span>
                        <Badge variant="outline" className="font-mono text-[9px] h-4 border-border/50">R{s.replica_id}</Badge>
                      </div>
                      <div className="space-y-2.5">
                        <div className="flex items-center justify-between text-[10px] font-bold text-muted-foreground/60 uppercase tracking-widest">
                          <span>KV CACHE</span>
                          <span className={cn("font-mono", isOverloaded ? "text-destructive" : hasKv ? "text-primary" : "text-muted-foreground")}>
                            {kvPct != null ? `${kvPct}%` : 'n/a'}
                          </span>
                        </div>
                        <Progress value={kvPct ?? 0} className="h-1.5 bg-white/5" indicatorClassName={isOverloaded ? "bg-destructive" : "bg-primary"} />
                      </div>
                      <div className="grid grid-cols-2 gap-4 pt-1">
                          <div className="flex flex-col">
                             <span className="text-[9px] font-bold text-muted-foreground/50 uppercase">PENDING</span>
                             <span className="text-xs font-mono font-bold text-foreground">{s.pending_requests}</span>
                          </div>
                          {s.prefix_cache_hit_rate != null && (
                            <div className="flex flex-col text-right">
                                <span className="text-[9px] font-bold text-muted-foreground/50 uppercase">CACHE HIT</span>
                                <span className="text-xs font-mono font-bold text-success">{(s.prefix_cache_hit_rate * 100).toFixed(1)}%</span>
                            </div>
                          )}
                      </div>
                    </div>
                  );
                })}
              </div>
        </div>
      )}
      
      <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
        <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
            <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">{t('inference.activeEndpoints')}</h3>
        </div>
        <Table>
            <TableHeader className="bg-black/20">
                <TableRow className="border-border/50 hover:bg-transparent">
                    <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">Identity</TableHead>
                    <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Replica</TableHead>
                    <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Computing Node</TableHead>
                    <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">API Protocol</TableHead>
                    <TableHead className="text-[10px] uppercase font-bold text-muted-foreground text-right pr-6">Status</TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {!overview || overview.endpoints.length === 0 ? (
                    <TableRow>
                        <TableCell colSpan={5} className="h-40 text-center text-[10px] font-mono uppercase tracking-widest text-muted-foreground opacity-50">
                            {t('inference.noActiveEndpoints')}
                        </TableCell>
                    </TableRow>
                ) : (
                    overview.endpoints.map((ep) => (
                        <TableRow key={`${ep.model_uid}-${ep.replica_id}`} className="border-border/40 hover:bg-white/5 transition-colors">
                            <TableCell className="px-6 py-4 font-mono text-xs font-bold text-foreground">{ep.model_uid}</TableCell>
                            <TableCell className="font-mono text-xs">R{ep.replica_id}</TableCell>
                            <TableCell>
                                <div className="flex items-center gap-2">
                                    <Server className="h-3 w-3 text-muted-foreground" />
                                    <span className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground">{ep.node_id}</span>
                                </div>
                            </TableCell>
                            <TableCell>
                                <Badge variant="outline" className="font-mono text-[9px] uppercase border-border/50">{ep.api_flavor}</Badge>
                            </TableCell>
                            <TableCell className="text-right pr-6">
                                <div className="flex items-center justify-end gap-2">
                                    <div className={cn("w-1.5 h-1.5 rounded-full", ep.status.toLowerCase().includes('run') ? "bg-success" : "bg-warning animate-pulse")} />
                                    <span className={cn("text-[9px] font-bold uppercase tracking-widest", ep.status.toLowerCase().includes('run') ? "text-success" : "text-warning")}>
                                        {ep.status}
                                    </span>
                                </div>
                            </TableCell>
                        </TableRow>
                    ))
                )}
            </TableBody>
        </Table>
      </div>
    </div>
  );
}

function MetricCard({ label, value, icon: Icon, color = "text-foreground" }: any) {
    return (
        <div className="bg-card/40 backdrop-blur-xl border border-border p-5 rounded-xl rim-light relative overflow-hidden group">
            <div className="flex items-center justify-between mb-4">
                <span className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">{label}</span>
                <Icon className="h-4 w-4 text-muted-foreground/50 group-hover:text-primary transition-colors" />
            </div>
            <span className={cn("text-2xl font-mono font-bold tracking-tighter", color)}>{value}</span>
        </div>
    )
}
