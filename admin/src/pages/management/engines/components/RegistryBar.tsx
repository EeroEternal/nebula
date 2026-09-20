import { App, Button, Input, Popover, Table, Tooltip } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import {
  Check,
  CloudDownload,
  Link2,
  Loader2,
  Plus,
  Save,
  Settings2,
  Trash2,
  X,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';

import {
  EngineRegistryConfig,
  getEngineRegistry,
  probeEngineImage,
  updateEngineRegistry,
} from '@/services/engineImages';
import { stringifyDetail } from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';

import {
  BUILTIN_ENGINE_PRESETS,
  compareEngines,
  engineTagLabel,
  engineTagTone,
} from '../utils';
import StatusTag from './StatusTag';

type Props = {
  onSaved?: (cfg: EngineRegistryConfig) => void;
  /** 从镜像源拉取远程 tag（与引擎仓库「更新仓库」相同） */
  onSyncRemote?: () => void | Promise<void>;
  syncingRemote?: boolean;
  /** 外部重置/保存后递增，触发重新加载 */
  reloadToken?: number;
};

type RepoRow = { engine: string; repo: string };

/** 用当前 Registry + 仓库路径拼探测镜像（复用 /engine-images/probe） */
const buildProbeImage = (registryHost: string, repoPath: string) => {
  const host = (registryHost || '').trim().replace(/\/$/, '') || 'docker.io';
  let repo = (repoPath || '').trim().replace(/^\/+/, '');
  if (!repo) repo = 'library/hello-world';
  const first = repo.split('/')[0] || '';
  const repoHasHost = first.includes('.') || first.includes(':');
  if (repoHasHost) {
    return repo.includes(':') ? repo : `${repo}:latest`;
  }
  if (
    host === 'docker.io' ||
    host === 'index.docker.io' ||
    host === 'registry-1.docker.io'
  ) {
    return repo.includes(':') ? repo : `${repo}:latest`;
  }
  const full = `${host}/${repo}`;
  return full.includes(':') ? full : `${full}:latest`;
};

const rowsFromConfig = (data?: EngineRegistryConfig | null): RepoRow[] => {
  const repos = data?.repos || {};
  const keys = Object.keys(repos);
  if (!keys.length) return [];
  return keys
    .sort(compareEngines)
    .map((engine) => ({ engine, repo: repos[engine] || '' }));
};

const presetOf = (engineKey: string) =>
  BUILTIN_ENGINE_PRESETS.find(
    (p) => p.engine === (engineKey || '').trim().toLowerCase(),
  );

const sectionShell =
  'rounded-lg border border-[color:var(--c-border-light)] bg-[var(--c-surface)] shadow-card px-5 py-5 flex flex-col gap-4 w-full';

/** 来源配置页：Registry / 映射 / 已启动引擎 三区分离 */
const RegistryBar: React.FC<Props> = ({
  onSaved,
  onSyncRemote,
  syncingRemote,
  reloadToken = 0,
}) => {
  const { message } = App.useApp();
  const [registry, setRegistry] = useState('docker.io');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [hasPassword, setHasPassword] = useState(false);
  const [rows, setRows] = useState<RepoRow[]>([]);
  const [source, setSource] = useState<string>('env');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [probing, setProbing] = useState(false);
  const [customOpen, setCustomOpen] = useState(false);
  const [customName, setCustomName] = useState('');
  const [editingEngine, setEditingEngine] = useState<string | null>(null);
  const [draftRepo, setDraftRepo] = useState('');

  const usedEngines = useMemo(
    () => new Set(rows.map((r) => r.engine.trim().toLowerCase()).filter(Boolean)),
    [rows],
  );

  const sourceLabel =
    source === 'file'
      ? String(lGet('models.engines.registrySourceFile'))
      : String(lGet('models.engines.registrySourceEnv'));

  const load = async () => {
    setLoading(true);
    try {
      const res = await getEngineRegistry();
      if (!res?.success) {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.registryLoadFailed')),
        );
        return;
      }
      applyLoaded(res.data as EngineRegistryConfig);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setCustomOpen(false);
    setCustomName('');
    setEditingEngine(null);
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reloadToken]);

  const toggleBuiltin = (engine: string) => {
    const key = engine.toLowerCase();
    if (usedEngines.has(key)) {
      setRows((prev) => prev.filter((r) => r.engine.toLowerCase() !== key));
      if (editingEngine === key) setEditingEngine(null);
      return;
    }
    const preset = presetOf(key);
    const next: RepoRow = {
      engine: key,
      repo: (preset?.repo || '').trim(),
    };
    setRows((prev) =>
      [...prev, next].sort((a, b) => compareEngines(a.engine, b.engine)),
    );
    if (!next.repo) {
      setEditingEngine(key);
      setDraftRepo('');
    }
  };

  const addCustom = () => {
    const name = customName.trim().toLowerCase();
    if (!name) {
      message.warning(lGet('models.engines.registryCustomNameRequired'));
      return;
    }
    if (usedEngines.has(name)) {
      message.info(lGet('models.engines.registryAlreadyAdded'));
      return;
    }
    const preset = presetOf(name);
    setRows((prev) =>
      [...prev, { engine: name, repo: (preset?.repo || '').trim() }].sort(
        (a, b) => compareEngines(a.engine, b.engine),
      ),
    );
    setCustomOpen(false);
    setCustomName('');
    if (!preset?.repo) {
      setEditingEngine(name);
      setDraftRepo('');
    }
  };

  const openRepoEditor = (engine: string, repo: string) => {
    setEditingEngine(engine);
    setDraftRepo(repo || '');
  };

  const applyRepoEdit = () => {
    if (!editingEngine) return;
    const key = editingEngine;
    const repo = draftRepo.trim();
    setRows((prev) =>
      prev.map((r) => (r.engine.toLowerCase() === key ? { ...r, repo } : r)),
    );
    setEditingEngine(null);
  };

  const handleProbeLink = async () => {
    const host = registry.trim() || 'docker.io';
    const repo =
      rows.map((r) => r.repo.trim()).find(Boolean) ||
      presetOf('vllm')?.repo ||
      'library/hello-world';
    const image = buildProbeImage(host, repo);
    setProbing(true);
    try {
      const res = await probeEngineImage({ image });
      const data = (res?.data || {}) as { ok?: boolean; message?: string };
      if (res?.success && data.ok) {
        message.success(
          String(
            lGet('models.engines.registryProbeOk', undefined, {
              host,
              detail: data.message || 'ok',
            }),
          ),
        );
      } else {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            data.message ||
            String(lGet('models.engines.registryProbeFailed')),
        );
      }
    } finally {
      setProbing(false);
    }
  };

  const applyLoaded = (data: EngineRegistryConfig) => {
    setRegistry(data.registry || 'docker.io');
    setUsername(data.username || '');
    setPassword('');
    setHasPassword(!!data.has_password);
    setRows(
      rowsFromConfig(data).map((row) => ({
        ...row,
        repo: row.repo || presetOf(row.engine)?.repo || '',
      })),
    );
    setSource(data.source || 'env');
    onSaved?.(data);
  };

  const handleSave = async () => {
    const missing = rows
      .filter((r) => !r.repo.trim())
      .map((r) => engineTagLabel(r.engine));
    if (missing.length) {
      message.warning(
        lGet('models.engines.registryNeedRepo', undefined, {
          engines: missing.join(', '),
        }),
      );
      return;
    }
    if (!rows.length) {
      message.warning(lGet('models.engines.registryNeedRepos'));
      return;
    }
    const repos: Record<string, string> = {};
    for (const row of rows) {
      const engine = row.engine.trim().toLowerCase();
      const repo = row.repo.trim();
      if (!engine || !repo) continue;
      repos[engine] = repo;
    }
    setSaving(true);
    try {
      const res = await updateEngineRegistry({
        registry: registry.trim() || 'docker.io',
        repos,
        username: username.trim() || undefined,
        // 空密码表示保留已保存密钥
        password: password || undefined,
      });
      if (!res?.success) {
        message.error(
          stringifyDetail(res?.data?.detail) ||
            String(lGet('models.engines.registrySaveFailed')),
        );
        return;
      }
      const data = res.data as EngineRegistryConfig;
      setSource(data.source || 'file');
      setRows(rowsFromConfig(data));
      setUsername(data.username || '');
      setPassword('');
      setHasPassword(!!data.has_password);
      message.success(lGet('models.engines.registrySaved'));
      onSaved?.(data);
    } finally {
      setSaving(false);
    }
  };

  const columns: ColumnsType<RepoRow> = [
    {
      title: l('models.engines.engine'),
      dataIndex: 'engine',
      width: 160,
      render: (engine: string) => (
        <StatusTag tone={engineTagTone(engine)} dot={false}>
          {engineTagLabel(engine)}
        </StatusTag>
      ),
    },
    {
      title: l('models.engines.registryRepo'),
      dataIndex: 'repo',
      ellipsis: true,
      render: (repo: string) => {
        const needRepo = !String(repo || '').trim();
        return (
          <span
            className={[
              'font-mono text-[13px]',
              needRepo ? 'text-warning' : 'text-secondary',
            ].join(' ')}
          >
            {needRepo ? l('models.engines.registryRepoPending') : repo}
          </span>
        );
      },
    },
    {
      title: l('global.actions.action'),
      key: 'actions',
      width: 100,
      align: 'right',
      render: (_, row) => {
        const key = row.engine.toLowerCase();
        const needRepo = !row.repo.trim();
        const editing = editingEngine === key;
        return (
          <div className="flex items-center justify-end gap-1">
            <Popover
              trigger="click"
              open={editing}
              onOpenChange={(v) => {
                if (v) openRepoEditor(key, row.repo);
                else setEditingEngine(null);
              }}
              placement="left"
              title={l('models.engines.registryRepo')}
              content={
                <div className="flex flex-col gap-2.5 w-[320px]">
                  <Input
                    className="font-mono text-xs h-9"
                    placeholder="org/image-name"
                    value={draftRepo}
                    onChange={(e) => setDraftRepo(e.target.value)}
                    onPressEnter={applyRepoEdit}
                  />
                  <div className="flex justify-end gap-2">
                    <Button type="primary" size="small" onClick={applyRepoEdit}>
                      {l('models.engines.registryApplyRepo')}
                    </Button>
                    <Button size="small" onClick={() => setEditingEngine(null)}>
                      {l('models.engines.cancel')}
                    </Button>
                  </div>
                </div>
              }
            >
              <Button
                type="text"
                size="small"
                icon={<Settings2 size={16} />}
                aria-label={String(lGet('models.engines.registryRepo'))}
                className={needRepo ? 'text-warning' : undefined}
              />
            </Popover>
            <Button
              type="text"
              size="small"
              danger
              icon={<Trash2 size={16} />}
              aria-label={String(lGet('models.engines.registryRemoveEngine'))}
              onClick={() =>
                setRows((prev) =>
                  prev.filter((r) => r.engine.toLowerCase() !== key),
                )
              }
            />
          </div>
        );
      },
    },
  ];

  return (
    <div className="flex flex-col gap-4 w-full">
      {/* 1. Registry 地址 / 账号 + 保存配置 + 更新仓库 */}
      <div className={sectionShell}>
        <div className="flex flex-wrap items-start gap-x-3 gap-y-3">
          <div className="flex flex-col gap-1 min-w-[220px] flex-1 max-w-md">
            <span className="text-[12px] font-semibold text-secondary">
              {l('models.engines.registryHost')}
            </span>
            <Input
              value={registry}
              onChange={(e) => setRegistry(e.target.value)}
              placeholder="docker.io"
              disabled={loading}
              className="font-mono !h-9"
            />
            <span className="text-[11px] text-muted leading-snug pl-0.5">
              {lGet('models.engines.registrySourceHint', undefined, {
                source: sourceLabel,
              })}
            </span>
          </div>
          <div className="flex flex-col gap-1 min-w-[140px] w-[160px]">
            <span className="text-[12px] font-semibold text-secondary">
              {l('models.engines.registryUsername')}
            </span>
            <Input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder={lGet('models.engines.registryUsernamePh')}
              disabled={loading}
              className="!h-9"
              autoComplete="username"
            />
          </div>
          <div className="flex flex-col gap-1 min-w-[140px] w-[180px]">
            <span className="text-[12px] font-semibold text-secondary">
              {l('models.engines.registryPassword')}
            </span>
            <Input.Password
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={
                hasPassword
                  ? String(lGet('models.engines.registryPasswordKeep'))
                  : String(lGet('models.engines.registryPasswordPh'))
              }
              disabled={loading}
              className="!h-9"
              autoComplete="new-password"
            />
          </div>
          <div className="flex flex-wrap items-center gap-2 shrink-0 ml-auto pt-5">
            <Tooltip title={lGet('models.engines.registryProbeHint')}>
              <Button
                icon={<Link2 size={16} />}
                loading={probing}
                disabled={loading || saving || !!syncingRemote}
                onClick={() => void handleProbeLink()}
              >
                {l('models.engines.registryProbe')}
              </Button>
            </Tooltip>
            <Button
              icon={<Save size={16} />}
              loading={saving || loading}
              disabled={!!syncingRemote || probing}
              onClick={handleSave}
            >
              {l('models.engines.registrySave')}
            </Button>
            {onSyncRemote ? (
              <Tooltip
                title={
                  syncingRemote
                    ? lGet('models.engines.remoteSyncProgressReopenHint')
                    : lGet('models.engines.remoteSyncHint')
                }
              >
                <Button
                  type="primary"
                  icon={
                    syncingRemote ? (
                      <Loader2 size={16} className="animate-spin" />
                    ) : (
                      <CloudDownload size={16} />
                    )
                  }
                  disabled={loading || saving || probing}
                  onClick={() => void onSyncRemote()}
                >
                  {l('models.engines.remoteSync')}
                </Button>
              </Tooltip>
            ) : null}
          </div>
        </div>
      </div>

      {/* 2. 引擎仓库映射 */}
      <div className={sectionShell}>
        <div className="flex items-start justify-between gap-3">
          <div className="flex flex-col gap-1 min-w-0">
            <span className="text-[14px] font-semibold text-default">
              {l('models.engines.registryMappingTitle')}
            </span>
            <span className="text-[12px] text-muted leading-relaxed">
              {l('models.engines.registrySelectHint')}
            </span>
          </div>
          <Button
            icon={<Plus size={14} />}
            disabled={loading}
            onClick={() => setCustomOpen(true)}
            className="shrink-0"
          >
            {l('models.engines.registryAddCustom')}
          </Button>
        </div>

        <div className="flex flex-wrap gap-2.5">
          {BUILTIN_ENGINE_PRESETS.map((p) => {
            const selected = usedEngines.has(p.engine);
            return (
              <button
                key={p.engine}
                type="button"
                disabled={loading}
                onClick={() => toggleBuiltin(p.engine)}
                className={[
                  'inline-flex items-center gap-1.5 rounded-full border px-3.5 py-2 text-[13px] font-medium transition-colors min-h-[36px]',
                  selected
                    ? 'border-[color:var(--c-primary)] bg-[color-mix(in_srgb,var(--c-primary)_10%,white)] text-navy'
                    : 'border-[color:var(--c-border)] bg-card text-secondary hover:border-[color:var(--c-primary)] hover:text-default',
                ].join(' ')}
              >
                {selected ? <Check size={14} className="text-primary" /> : null}
                {p.label}
              </button>
            );
          })}
        </div>

        {customOpen ? (
          <div className="flex flex-wrap items-center gap-2.5 rounded-xl border border-dashed border-[color:var(--c-border)] bg-[var(--c-surface-2)] px-4 py-3 min-h-[56px]">
            <Input
              className="w-full max-w-[280px] h-9"
              placeholder={l('models.engines.registryCustomPlaceholder')}
              value={customName}
              disabled={loading}
              onChange={(e) => setCustomName(e.target.value)}
              onPressEnter={addCustom}
            />
            <Button type="primary" onClick={addCustom}>
              {l('models.engines.registryAddEngine')}
            </Button>
            <Button
              type="text"
              icon={<X size={14} />}
              onClick={() => {
                setCustomOpen(false);
                setCustomName('');
              }}
            />
          </div>
        ) : null}
      </div>

      {/* 3. 已启动引擎（表格） */}
      <div className={sectionShell}>
        <div className="flex items-center justify-between gap-3">
          <span className="text-[14px] font-semibold text-default">
            {l('models.engines.registryStartedTitle')}
          </span>
          <span className="text-[12px] text-muted">
            {lGet('models.engines.registryEnabledCount', undefined, {
              count: rows.length,
            })}
          </span>
        </div>
        <Table<RepoRow>
          rowKey={(r) => r.engine.toLowerCase()}
          size="middle"
          loading={loading}
          columns={columns}
          dataSource={rows}
          pagination={false}
          locale={{
            emptyText: l('models.engines.registryEmptySelected'),
          }}
        />
      </div>
    </div>
  );
};

export default RegistryBar;
