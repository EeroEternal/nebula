import { useState } from "react";
import { NavLink, useLocation } from "react-router-dom";
import {
    LayoutDashboard, Box, Server, Settings, HelpCircle, MessageSquare,
    ChevronRight, ChevronDown, Activity, Cpu, Shield, BookOpen, Layers, Zap, Network
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";

const Sidebar = () => {
    const { t } = useI18n();
    const location = useLocation();
    const pathname = location.pathname;

    const menuItems = [
        { id: 'dashboard', icon: LayoutDashboard, label: t('nav.dashboard'), path: '/' },
        { id: 'models', icon: Box, label: t('nav.models'), path: '/models' },
        { id: 'inference', icon: Activity, label: t('nav.inference'), path: '/inference' },
        { id: 'gateway', icon: Shield, label: t('nav.gateway'), path: '/inference/gateway' },
        { id: 'endpoints', icon: Cpu, label: t('nav.endpoints'), path: '/endpoints' },
        { id: 'cells', icon: Network, label: t('nav.cells'), path: '/cells' },
        { id: 'governance', icon: Shield, label: t('nav.governance'), path: '/governance' },
    ];

    const infrastructureItems = [
        { id: 'nodes', icon: Server, label: t('nav.nodes'), path: '/infrastructure/nodes' },
        { id: 'images', icon: Zap, label: t('nav.images'), path: '/infrastructure/images' },
        { id: 'templates', icon: Layers, label: t('nav.templates'), path: '/infrastructure/templates' },
    ];

    const resourceItems = [
        { id: 'model-catalog', icon: BookOpen, label: t('nav.catalog'), path: '/resources/model-catalog' },
        { id: 'model-library', icon: Layers, label: t('nav.library'), path: '/resources/model-library' },
        { id: 'audit', icon: Shield, label: t('nav.audit'), path: '/resources/audit' },
    ];

    const systemItems = [
        { id: 'settings', icon: Settings, label: t('nav.settings'), path: '/system/settings' },
        { icon: HelpCircle, label: t('nav.help'), path: '/help' },
        { icon: MessageSquare, label: t('nav.feedback'), path: '/feedback' },
    ];

    const [menuOpen, setMenuOpen] = useState(true);
    const [infraOpen, setInfraOpen] = useState(true);
    const [resourcesOpen, setResourcesOpen] = useState(true);
    const [systemOpen, setSystemOpen] = useState(true);

    const NavItem = ({ item }: { item: any }) => (
        <NavLink
            to={item.path}
            className={({ isActive }) => cn(
                "flex items-center gap-3 w-full px-3 py-2 rounded-md text-sm font-medium transition-all duration-200",
                isActive
                    ? "bg-primary text-primary-foreground rim-light"
                    : "text-muted-foreground hover:text-foreground hover:bg-white/5"
            )}
        >
            <item.icon className={cn("h-[18px] w-[18px]", pathname === item.path ? "animate-signal" : "")} />
            {item.label}
        </NavLink>
    );

    return (
        <aside className="w-64 shrink-0 bg-card/40 backdrop-blur-xl border-r border-border flex flex-col">
            <div className="px-6 py-8 flex items-center gap-3 border-b border-border/50">
                <div className="w-8 h-8 rounded-lg bg-primary flex items-center justify-center rim-light">
                    <Activity className="h-5 w-5 text-primary-foreground" />
                </div>
                <h1 className="text-xl font-bold text-foreground tracking-tight font-mono">NEBULA</h1>
            </div>

            <nav className="flex-1 px-4 py-6 overflow-y-auto space-y-6">
                <div>
                    <div className="flex items-center justify-between px-3 mb-2">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.workbench')}</p>
                        <button onClick={() => setMenuOpen(!menuOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
                            {menuOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                        </button>
                    </div>
                    {menuOpen && (
                        <div className="space-y-1">
                            {menuItems.map((item) => <NavItem key={item.id} item={item} />)}
                        </div>
                    )}
                </div>

                <div>
                    <div className="flex items-center justify-between px-3 mb-2">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.infrastructure')}</p>
                        <button onClick={() => setInfraOpen(!infraOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
                            {infraOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                        </button>
                    </div>
                    {infraOpen && (
                        <div className="space-y-1">
                            {infrastructureItems.map((item) => <NavItem key={item.id} item={item} />)}
                        </div>
                    )}
                </div>

                <div>
                    <div className="flex items-center justify-between px-3 mb-2">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.resources')}</p>
                        <button onClick={() => setResourcesOpen(!resourcesOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
                            {resourcesOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                        </button>
                    </div>
                    {resourcesOpen && (
                        <div className="space-y-1">
                            {resourceItems.map((item) => <NavItem key={item.id} item={item} />)}
                        </div>
                    )}
                </div>

                <div>
                    <div className="flex items-center justify-between px-3 mb-2">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.system')}</p>
                        <button onClick={() => setSystemOpen(!systemOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
                            {systemOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                        </button>
                    </div>
                    {systemOpen && (
                        <div className="space-y-1">
                            {systemItems.map((item) => <NavItem key={item.label} item={item} />)}
                        </div>
                    )}
                </div>
            </nav>

            <div className="px-6 py-4 border-t border-border/50 bg-white/5">
                <div className="flex items-center justify-between">
                    <p className="text-[10px] font-mono text-muted-foreground">VERSION 0.1.1</p>
                    <div className="flex gap-2">
                        <div className="w-1.5 h-1.5 rounded-full bg-success animate-pulse" />
                        <p className="text-[10px] font-mono text-success uppercase tracking-widest">Connected</p>
                    </div>
                </div>
            </div>
        </aside>
    );
};

export default Sidebar;
