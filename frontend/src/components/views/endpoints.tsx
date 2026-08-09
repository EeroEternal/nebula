import { Globe, Server, Activity, Shield, Link } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { useClusterOverview } from "@/hooks/useClusterOverview"
import { cn } from "@/lib/utils"
import { endpointStatusTone } from "@/lib/endpoint-status"

export function EndpointsView() {
    const { data: overview } = useClusterOverview()

    return (
        <div className="space-y-8 animate-in fade-in duration-500">
            <div className="flex justify-between items-end">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">API Endpoints</h2>
                    <p className="text-muted-foreground mt-2 flex items-center gap-2">
                        <Globe className="h-4 w-4 text-primary" />
                        Access points and protocol interfaces for model inference
                    </p>
                </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
                <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl rim-light">
                    <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">Total Endpoints</p>
                    <h3 className="text-2xl font-mono font-bold text-foreground">{overview?.endpoints.length || 0}</h3>
                    <div className="mt-4 flex items-center gap-2 text-[10px] text-muted-foreground uppercase font-bold">
                        <Activity className="h-3 w-3 text-success" /> Load Balanced
                    </div>
                </div>
                <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl rim-light">
                    <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">Active Protocols</p>
                    <h3 className="text-2xl font-mono font-bold text-foreground">2</h3>
                    <div className="mt-4 flex gap-2">
                        <Badge className="bg-primary/10 text-primary border-primary/20 text-[9px]">REST/OAI</Badge>
                        <Badge className="bg-primary/10 text-primary border-primary/20 text-[9px]">GRPC</Badge>
                    </div>
                </div>
                <div className="bg-card/40 backdrop-blur-xl border border-border p-6 rounded-xl rim-light">
                    <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">Mesh Health</p>
                    <h3 className="text-2xl font-mono font-bold text-success">NOMINAL</h3>
                    <div className="mt-4 flex items-center gap-2 text-[10px] text-muted-foreground uppercase font-bold">
                        <Shield className="h-3 w-3 text-success" /> Traffic Encrypted
                    </div>
                </div>
            </div>

            <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
                <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
                    <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">Endpoint Distribution</h3>
                </div>
                <Table>
                    <TableHeader className="bg-black/20">
                        <TableRow className="border-border/50 hover:bg-transparent">
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">Identity</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Computing Resource</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Interface</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">Target URL</TableHead>
                            <TableHead className="text-right text-[10px] uppercase font-bold text-muted-foreground pr-6">Connectivity</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {!overview || overview.endpoints.length === 0 ? (
                            <TableRow>
                                <TableCell colSpan={5} className="h-64 text-center text-[10px] font-mono uppercase tracking-widest text-muted-foreground opacity-50">
                                    No active endpoints detected in the mesh
                                </TableCell>
                            </TableRow>
                        ) : (
                            overview.endpoints.map((ep) => (
                                <TableRow key={`${ep.model_uid}-${ep.replica_id}`} className="border-border/40 hover:bg-white/5 transition-colors">
                                    <TableCell className="px-6 py-5">
                                        <div className="flex flex-col gap-1">
                                            <span className="font-mono text-sm font-bold text-foreground uppercase">{ep.model_uid}</span>
                                            <span className="text-[9px] font-mono text-muted-foreground uppercase tracking-widest">REPLICA ID: {ep.replica_id}</span>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <div className="flex items-center gap-2">
                                            <Server className="h-3.5 w-3.5 text-muted-foreground" />
                                            <span className="text-[11px] font-mono font-bold text-foreground uppercase">{ep.node_id}</span>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <Badge variant="outline" className="font-mono text-[9px] border-border/50 uppercase text-muted-foreground">{ep.api_flavor}</Badge>
                                    </TableCell>
                                    <TableCell>
                                        <div className="flex items-center gap-2 group cursor-pointer">
                                            <Link className="h-3 w-3 text-primary opacity-50 group-hover:opacity-100 transition-opacity" />
                                            <span className="text-[10px] font-mono text-muted-foreground group-hover:text-foreground transition-colors truncate max-w-[300px]">
                                                {ep.base_url || ep.grpc_target || "INTERNAL_ROUTING_ONLY"}
                                            </span>
                                        </div>
                                    </TableCell>
                                    <TableCell className="text-right pr-6">
                                        <div className="flex flex-col items-end gap-1">
                                            <div className="flex items-center justify-end gap-2">
                                                {(() => {
                                                    const tone = endpointStatusTone(ep.status)
                                                    return (
                                                        <>
                                                            <div className={cn(
                                                                "w-1.5 h-1.5 rounded-full",
                                                                tone === "success" ? "bg-success animate-signal" : tone === "destructive" ? "bg-destructive animate-pulse" : "bg-warning"
                                                            )} />
                                                            <span className={cn(
                                                                "text-[9px] font-bold uppercase tracking-widest",
                                                                tone === "success" ? "text-success" : tone === "destructive" ? "text-destructive" : "text-warning"
                                                            )}>
                                                                {ep.status}
                                                            </span>
                                                        </>
                                                    )
                                                })()}
                                            </div>
                                            {ep.status_detail && (
                                                <span className="text-[9px] text-muted-foreground max-w-[220px] truncate text-right" title={ep.status_detail}>
                                                    {ep.status_detail}
                                                </span>
                                            )}
                                        </div>
                                    </TableCell>
                                </TableRow>
                            ))
                        )}
                    </TableBody>
                </Table>
            </div>
        </div>
    )
}
