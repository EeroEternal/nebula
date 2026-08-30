import { createBrowserRouter, Navigate } from 'react-router-dom'
import { DashboardLayout } from '@/layouts/DashboardLayout'
import { lazy, Suspense } from 'react'
import { SuspenseWrapper } from './SuspenseWrapper'

// Code Splitting for views
const DashboardView = lazy(() => import('@/components/views/dashboard').then(m => ({ default: m.DashboardView })))
const ModelsView = lazy(() => import('@/components/views/models').then(m => ({ default: m.ModelsView })))
const NodesView = lazy(() => import('@/components/views/nodes').then(m => ({ default: m.NodesView })))
const GatewayView = lazy(() => import('@/components/views/gateway').then(m => ({ default: m.GatewayView })))
const InferenceView = lazy(() => import('@/components/views/inference').then(m => ({ default: m.InferenceView })))
const ImagesView = lazy(() => import('@/components/views/images').then(m => ({ default: m.ImagesView })))
const TemplatesView = lazy(() => import('@/components/views/templates').then(m => ({ default: m.TemplatesView })))
const ModelCatalogView = lazy(() => import('@/components/views/model-catalog').then(m => ({ default: m.ModelCatalogView })))
const ModelLibraryView = lazy(() => import('@/components/views/model-library').then(m => ({ default: m.ModelLibraryView })))
const AuditView = lazy(() => import('@/components/views/audit').then(m => ({ default: m.AuditView })))
const SettingsView = lazy(() => import('@/components/views/settings').then(m => ({ default: m.SettingsView })))
const EndpointsView = lazy(() => import('@/components/views/endpoints').then(m => ({ default: m.EndpointsView })))
const GovernanceView = lazy(() => import('@/components/views/governance').then(m => ({ default: m.GovernanceView })))
const LoginView = lazy(() => import('@/components/views/login').then(m => ({ default: m.LoginView })))

export const router = createBrowserRouter([
  {
    path: '/',
    element: <DashboardLayout />,
    children: [
      { index: true, element: <SuspenseWrapper><DashboardView /></SuspenseWrapper> },
      { path: 'models', element: <SuspenseWrapper><ModelsView /></SuspenseWrapper> },
      { path: 'inference', element: <SuspenseWrapper><InferenceView /></SuspenseWrapper> },
      { path: 'inference/gateway', element: <SuspenseWrapper><GatewayView /></SuspenseWrapper> },
      { path: 'endpoints', element: <SuspenseWrapper><EndpointsView /></SuspenseWrapper> },
      { path: 'governance', element: <SuspenseWrapper><GovernanceView /></SuspenseWrapper> },
      { path: 'infrastructure/nodes', element: <SuspenseWrapper><NodesView /></SuspenseWrapper> },
      { path: 'infrastructure/images', element: <SuspenseWrapper><ImagesView /></SuspenseWrapper> },
      { path: 'infrastructure/templates', element: <SuspenseWrapper><TemplatesView /></SuspenseWrapper> },
      { path: 'resources/model-catalog', element: <SuspenseWrapper><ModelCatalogView /></SuspenseWrapper> },
      { path: 'resources/model-library', element: <SuspenseWrapper><ModelLibraryView /></SuspenseWrapper> },
      { path: 'resources/audit', element: <SuspenseWrapper><AuditView /></SuspenseWrapper> },
      { path: 'system/settings', element: <SuspenseWrapper><SettingsView /></SuspenseWrapper> },
    ],
  },
  {
    path: '/login',
    element: (
      <Suspense fallback={<div className="h-screen w-screen bg-background"></div>}>
        <LoginView />
      </Suspense>
    ),
  },
  {
    path: '*',
    element: <Navigate to="/" replace />,
  },
])