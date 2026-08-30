import { useEffect, useState } from "react";
import { NavLink, useLocation } from "react-router-dom";
import {
    LayoutDashboard, Box, Server, Settings,
    ChevronRight, ChevronDown, Activity, Cpu, Shield, BookOpen, Layers, Zap, X
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/useI18n";

type SidebarProps = {
    mobileOpen?: boolean;
    onClose?: () => void;
};

type NavigationItem = {
    id: string;
    icon: LucideIcon;
    label: string;
    path: string;
};

const Sidebar = ({ mobileOpen = false, onClose = () => undefined }: SidebarProps) => {
    const { t } = useI18n();
    const location = useLocation();
    const pathname = location.pathname;

    const menuItems = [
        { id: 'dashboard', icon: LayoutDashboard, label: t('nav.dashboard'), path: '/' },
        { id: 'models', icon: Box, label: t('nav.models'), path: '/models' },
        { id: 'inference', icon: Activity, label: t('nav.inference'), path: '/inference' },
        { id: 'gateway', icon: Shield, label: t('nav.gateway'), path: '/inference/gateway' },
        { id: 'endpoints', icon: Cpu, label: t('nav.endpoints'), path: '/endpoints' },
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
    ];

    const [menuOpen, setMenuOpen] = useState(true);
    const [infraOpen, setInfraOpen] = useState(true);
    const [resourcesOpen, setResourcesOpen] = useState(true);
    const [systemOpen, setSystemOpen] = useState(true);

    useEffect(() => {
        onClose();
    }, [pathname, onClose]);

    const NavItem = ({ item }: { item: NavigationItem }) => (
        <NavLink
            to={item.path}
            onClick={onClose}
            className={({ isActive }) => cn(
                "flex w-full items-center gap-3 rounded-md px-3 py-1.5 text-sm font-medium transition-all duration-200",
                isActive
                    ? "bg-primary text-primary-foreground rim-light"
                    : "text-muted-foreground hover:text-foreground hover:bg-white/5"
            )}
        >
            <item.icon aria-hidden="true" className={cn("h-[18px] w-[18px] shrink-0", pathname === item.path ? "animate-signal" : "")} />
            <span className="truncate">{item.label}</span>
        </NavLink>
    );

    return (
        <>
            {mobileOpen && (
                <button
                    type="button"
                    aria-label={t('app.closeNavigation')}
                    className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm md:hidden"
                    onClick={onClose}
                />
            )}
            <aside className={cn(
                "fixed inset-y-0 left-0 z-50 flex h-screen w-72 shrink-0 flex-col border-r border-border bg-card/95 backdrop-blur-xl transition-transform duration-200 md:sticky md:top-0 md:z-30 md:w-64 md:translate-x-0 md:bg-card/40",
                mobileOpen ? "translate-x-0" : "-translate-x-full"
            )}>
            <div className="flex items-center justify-between border-b border-border/50 px-5 py-5 md:px-6 md:py-5">
                <div className="flex items-center gap-3">
                <div className="w-8 h-8 rounded-lg bg-primary flex items-center justify-center rim-light">
                    <Activity aria-hidden="true" className="h-5 w-5 text-primary-foreground" />
                </div>
                <h1 className="text-xl font-bold text-foreground tracking-tight font-mono">NEBULA</h1>
                </div>
                <button
                    type="button"
                    aria-label={t('app.closeNavigation')}
                    className="rounded-md p-2 text-muted-foreground hover:bg-white/5 hover:text-foreground md:hidden"
                    onClick={onClose}
                >
                    <X className="h-5 w-5" />
                </button>
            </div>

            <nav className="min-h-0 flex-1 space-y-3 overflow-y-auto px-4 py-4 md:space-y-3 md:overflow-y-visible" aria-label={t('app.primaryNavigation')}>
                <div>
                    <div className="mb-1.5 flex items-center justify-between px-3">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.workbench')}</p>
                        <button type="button" aria-label={t('app.toggleSection')} aria-expanded={menuOpen} onClick={() => setMenuOpen(!menuOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
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
                    <div className="mb-1.5 flex items-center justify-between px-3">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.infrastructure')}</p>
                        <button type="button" aria-label={t('app.toggleSection')} aria-expanded={infraOpen} onClick={() => setInfraOpen(!infraOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
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
                    <div className="mb-1.5 flex items-center justify-between px-3">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.resources')}</p>
                        <button type="button" aria-label={t('app.toggleSection')} aria-expanded={resourcesOpen} onClick={() => setResourcesOpen(!resourcesOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
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
                    <div className="mb-1.5 flex items-center justify-between px-3">
                        <p className="text-[11px] uppercase tracking-wider font-bold text-muted-foreground/60">{t('nav.system')}</p>
                        <button type="button" aria-label={t('app.toggleSection')} aria-expanded={systemOpen} onClick={() => setSystemOpen(!systemOpen)} className="text-muted-foreground/40 hover:text-foreground transition-colors">
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

            <div className="border-t border-border/50 bg-white/5 px-5 py-3">
                <div className="flex items-center justify-between">
                    <p className="text-[10px] font-mono text-muted-foreground">VERSION 1.8.0</p>
                    <div className="flex gap-2">
                        <div className="w-1.5 h-1.5 rounded-full bg-success animate-pulse" />
                        <p className="text-[10px] font-mono text-success uppercase tracking-widest">{t('app.connected')}</p>
                    </div>
                </div>
            </div>
            </aside>
        </>
    );
};

export default Sidebar;
