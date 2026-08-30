import { useMemo, useState } from 'react'
import { Clock3, RefreshCw, Shield, BarChart3, TrendingUp } from 'lucide-react'
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import type { GatewayTimePoint } from '@/lib/types'
import { useI18n } from '@/lib/useI18n'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { useGatewayStats } from '@/hooks/useGatewayStats'
import { cn } from '@/lib/utils'

const WINDOW_OPTIONS = ['5m', '15m', '1h', '6h', '24h'] as const

const latestValue = (points: GatewayTimePoint[]) => points[points.length - 1]?.value ?? 0
const toPct = (value: number) => `${(value * 100).toFixed(2)}%`
const fmtNumber = (value: number, digits = 2) => value.toLocaleString(undefined, { maximumFractionDigits: digits })

interface ChartDatum {
  name: string
  value: number
}

export function GatewayView() {
  const { t } = useI18n()
  const [windowValue, setWindowValue] = useState<(typeof WINDOW_OPTIONS)[number]>('15m')
  const { data, isLoading, isFetching, refetch } = useGatewayStats(windowValue)

  const { overview, traffic, reliability, protection, latency } = data || {}

  const trafficSummary = useMemo(() => {
    if (!traffic) return null
    return {
      total: latestValue(traffic.series.requests_total),
      s2xx: latestValue(traffic.series.responses_2xx),
      s4xx: latestValue(traffic.series.responses_4xx),
      s5xx: latestValue(traffic.series.responses_5xx),
    }
  }, [traffic])

  const reliabilitySummary = useMemo(() => {
    if (!reliability) return null
    return {
      retry: latestValue(reliability.series.retry_total),
      retrySuccess: latestValue(reliability.series.retry_success_total),
      connect: latestValue(reliability.series.upstream_error_connect),
      timeout: latestValue(reliability.series.upstream_error_timeout),
      up5xx: latestValue(reliability.series.upstream_error_5xx),
      other: latestValue(reliability.series.upstream_error_other),
    }
  }, [reliability])

  const latencySummary = useMemo(() => {
    if (!latency) return null
    return {
      p50: latestValue(latency.series.latency_p50_ms),
      p95: latestValue(latency.series.latency_p95_ms),
      p99: latestValue(latency.series.latency_p99_ms),
      ttft50: latestValue(latency.series.ttft_p50_ms),
      ttft95: latestValue(latency.series.ttft_p95_ms),
    }
  }, [latency])

  const chartData = useMemo(() => {
    if (!trafficSummary || !reliabilitySummary || !latencySummary || !protection) return { traffic: [], reliability: [], latency: [], protection: [] }
    
    return {
      traffic: [
        { name: '2xx', value: trafficSummary.s2xx },
        { name: '4xx', value: trafficSummary.s4xx },
        { name: '5xx', value: trafficSummary.s5xx },
      ],
      reliability: [
        { name: 'Retry', value: reliabilitySummary.retry },
        { name: 'Success', value: reliabilitySummary.retrySuccess },
        { name: 'Connect', value: reliabilitySummary.connect },
        { name: 'Timeout', value: reliabilitySummary.timeout },
        { name: 'Up 5xx', value: reliabilitySummary.up5xx },
      ],
      latency: [
        { name: 'P50', value: latencySummary.p50 },
        { name: 'P95', value: latencySummary.p95 },
        { name: 'P99', value: latencySummary.p99 },
        { name: 'TTFT 50', value: latencySummary.ttft50 },
        { name: 'TTFT 95', value: latencySummary.ttft95 },
      ],
      protection: [
        { name: 'Too Large', value: protection.request_too_large_count },
        { name: 'Skipped', value: protection.circuit_skipped_count },
        { name: 'Open', value: protection.circuit_open_count },
      ]
    }
  }, [trafficSummary, reliabilitySummary, latencySummary, protection])

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
        <div>
          <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('gateway.title')}</h2>
          <p className="text-muted-foreground mt-2 flex items-center gap-2">
            <Shield className="h-4 w-4 text-primary" />
            {t('gateway.subtitle')}
          </p>
        </div>
        <div className="flex items-center gap-3 bg-card/40 backdrop-blur-xl border border-border p-1.5 rounded-xl">
          <div className="flex items-center gap-1 bg-black/20 p-1 rounded-lg">
            {WINDOW_OPTIONS.map((option) => (
              <button
                key={option}
                onClick={() => setWindowValue(option)}
                className={cn(
                  "px-3 py-1.5 rounded-md text-[10px] font-bold uppercase tracking-wider transition-all",
                  windowValue === option 
                    ? "bg-primary text-primary-foreground shadow-sm" 
                    : "text-muted-foreground hover:text-foreground"
                )}
              >
                {option}
              </button>
            ))}
          </div>
          <Button 
            variant="ghost" 
            size="sm" 
            onClick={() => refetch()} 
            disabled={isLoading}
            className="h-9 w-9 p-0 hover:bg-white/10"
          >
            <RefreshCw className={cn("h-4 w-4", isFetching ? "animate-spin" : "")} />
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6">
        <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl rim-light relative overflow-hidden group">
            <div className="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
                <TrendingUp className="h-12 w-12" />
            </div>
            <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('gateway.rps')}</p>
            <h3 className="text-2xl font-mono font-bold text-foreground">{overview ? fmtNumber(overview.rps, 3) : '—'}</h3>
            <div className="flex items-center gap-1.5 mt-4">
                <div className="w-1.5 h-1.5 rounded-full bg-primary animate-signal" />
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest font-bold">{t('gateway.flowActive')}</p>
            </div>
        </div>

        <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl rim-light">
            <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('gateway.error5xx')}</p>
            <h3 className={cn("text-2xl font-mono font-bold", (overview?.error_5xx_ratio || 0) > 0.05 ? "text-destructive" : "text-foreground")}>
                {overview ? toPct(overview.error_5xx_ratio) : '—'}
            </h3>
            <div className="flex items-center gap-1.5 mt-4">
                <div className={cn("w-1.5 h-1.5 rounded-full", (overview?.error_5xx_ratio || 0) > 0.05 ? "bg-destructive" : "bg-success")} />
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest font-bold">{t('gateway.upstreamStatus')}</p>
            </div>
        </div>

        <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl rim-light">
            <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('gateway.retrySuccess')}</p>
            <h3 className="text-2xl font-mono font-bold text-foreground">{overview ? toPct(overview.retry_success_ratio) : '—'}</h3>
            <div className="flex items-center gap-1.5 mt-4">
                <Shield className="h-3 w-3 text-success" />
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest font-bold">{t('gateway.protectionActive')}</p>
            </div>
        </div>

        <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl rim-light">
             <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('gateway.circuitState')}</p>
            <h3 className={cn("text-2xl font-mono font-bold", (overview?.circuit_open_count || 0) > 0 ? "text-warning" : "text-success")}>
                 {overview?.circuit_open_count || 0 > 0 ? t('gateway.openDegraded') : t('gateway.closedNominal')}
            </h3>
            <div className="flex items-center gap-1.5 mt-4">
                <Clock3 className="h-3 w-3 text-muted-foreground" />
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest font-bold">{t('gateway.openBreaks', { count: overview?.circuit_open_count || 0 })}</p>
            </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <ChartCard title={t('gateway.traffic')} data={chartData.traffic} color="oklch(70% 0.18 190)" window={windowValue} />
        <ChartCard title={t('gateway.reliability')} data={chartData.reliability} color="oklch(75% 0.12 280)" window={windowValue} />
        <ChartCard title={t('gateway.latency')} data={chartData.latency} color="oklch(68% 0.22 150)" window={windowValue} unit="ms" />
        <ChartCard title={t('gateway.protection')} data={chartData.protection} color="oklch(60% 0.2 25)" window={windowValue} />
      </div>
    </div>
  )
}

function ChartCard({ title, data, color, window, unit = "" }: { title: string, data: ChartDatum[], color: string, window: string, unit?: string }) {
  return (
    <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl">
      <div className="flex items-center justify-between mb-8">
          <div className="flex items-center gap-2">
            <BarChart3 className="h-4 w-4 text-muted-foreground" />
            <h3 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">{title}</h3>
          </div>
           <Badge variant="outline" className="font-mono text-[9px] border-border/50 text-muted-foreground uppercase">{window} {useI18n().t('gateway.window')}</Badge>
      </div>
      <div className="h-64">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data}>
            <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="oklch(30% 0.05 260 / 0.2)" />
            <XAxis dataKey="name" axisLine={false} tickLine={false} tick={{ fontSize: 10, fill: "oklch(75% 0.02 260)" }} />
            <YAxis axisLine={false} tickLine={false} tick={{ fontSize: 10, fill: "oklch(75% 0.02 260)" }} />
            <Tooltip 
                cursor={{ fill: "oklch(100% 0 0 / 0.05)" }}
                contentStyle={{ backgroundColor: "oklch(22% 0.03 260)", border: "1px solid oklch(30% 0.05 260 / 0.5)", borderRadius: "8px", fontSize: "12px" }}
                itemStyle={{ color: "oklch(98% 0.01 260)" }}
                formatter={(value) => [`${fmtNumber(Number(value))}${unit}`, "Value"]}
            />
            <Bar dataKey="value" fill={color} radius={[4, 4, 0, 0]} barSize={40} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  )
}
