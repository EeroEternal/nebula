#!/usr/bin/env node
/**
 * One-shot: add static-ref keys that exist in code but in no locale file.
 * Values: zh-CN from call-site fallback; en-US written here; others copy en-US via sync.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { collectLocaleKeyMap } from './check-locales.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const localesRoot = join(__dirname, '..', 'src', 'locales');

const ZH = {
  'button.backBottom': '回到底部',
  'button.backTop': '回到顶部',
  'button.delete': '删除',
  'button.downScroll': '向下滚动',
  'button.refresh': '刷新',
  'button.startRefresh': '开始自动刷新',
  'button.stopRefresh': '停止自动刷新',
  'button.upScroll': '向上滚动',
  'button.wrap': '自动换行',
  'global.actions.copy': '复制',
  'global.actions.copy.success': '已复制',
  'global.actions.export': '导出',
  'global.actions.search': '查询',
  'global.fetch.failed': '加载失败',
  'global.fullScreen': '全屏',
  'global.fullScreen.exit': '退出全屏',
  'global.message.deleteFailed': '删除失败',
  'global.noData': '暂无数据',
  'models.deploy.manifest.empty': '暂无预览',
  'models.deploy.manifest.failed': '预览失败',
  'models.deploy.manifest.regenerate': '重新生成',
  'models.deploy.manifest.title': '启动 YAML',
  'models.downloads.nodeSupervisor': '控制节点',
  'models.engines.emptyLocalOnlyHint':
    '当前仅显示本机已有镜像。点击「更新仓库」可拉取远程 tag / 推荐项',
  'models.instances.manifest.failed': '加载失败',
  'models.instances.manifest.pending':
    '清单将在容器启动后写入；部署中或失败时可能暂无文件',
  'models.instances.modelName': '模型',
  'models.instances.modelUid': '实例 ID',
  'models.instances.replica': '副本',
  'models.instances.resources.gpuSlice': 'GPU 切片',
  'models.instances.viewLogs.containerNoLines':
    '已登记容器，但暂无日志行（可稍后自动刷新）',
  'models.repository.remoteSyncTimeout':
    '同步超时，请稍后刷新列表确认是否已完成',
  'monitor.cluster.live': '实时',
  'monitor.cluster.tabs': '集群监控子页',
  'monitor.hami.policy.auto': '自动（集群默认，只影响之后的新部署）',
  'monitor.hami.policy.binpack': '紧凑：尽量把任务塞到少量 GPU',
  'monitor.hami.policy.spread': '均衡：尽量把任务分散到不同 GPU',
  'monitor.instances.active': '进行中',
  'monitor.instances.bench.concurrency': '并发',
  'monitor.instances.bench.createdAt': '创建',
  'monitor.instances.bench.createdNone':
    '压测任务创建失败（实例需 READY；初始化中请稍后）',
  'monitor.instances.bench.prompts': '请求数',
  'monitor.instances.bench.start': '开始压测',
  'monitor.instances.bench.startMulti': '分散压测全部实例',
  'monitor.instances.filter': '筛选实例/模型',
  'monitor.instances.gpuUtilRatio': '显卡 util 占比',
  'monitor.instances.logs': '日志',
  'monitor.instances.pending': '排队',
  'monitor.instances.query.title30d': '近 30 天查询 · {uid}',
  'monitor.instances.resources': '瞬时负载',
  'monitor.instances.toManage': '去配置/扩缩',
  'monitor.instances.traces': '链路',
  'monitor.instances.usage.avgLatency': '平均延迟',
  'monitor.instances.usage.fromTraffic': '同源对话监控',
  'monitor.instances.usage.inputTokens': '输入 Token',
  'monitor.instances.usage.outputTokens': '输出 Token',
  'monitor.instances.usage.requests': '请求总数',
  'monitor.instances.usage.titleConv': '近 {days} 天对话用量',
  'monitor.instances.usage.totalTokens': '总 Token',
  'monitor.traces.exportCsv': '导出 CSV',
  'monitor.traces.live': '实时刷新',
  'monitor.traces.table.sourceIp': '来源 IP',
  'monitor.traces.table.status': '状态',
  'monitor.traces.table.tokensPerSecond': '每 Token 速度',
  'monitor.traces.table.ttft': '首 Token',
  'pages.welcome.alertMessage': '欢迎使用 PowerLLM',
};

const EN = {
  'button.backBottom': 'Jump to bottom',
  'button.backTop': 'Jump to top',
  'button.delete': 'Delete',
  'button.downScroll': 'Scroll down',
  'button.refresh': 'Refresh',
  'button.startRefresh': 'Start auto refresh',
  'button.stopRefresh': 'Stop auto refresh',
  'button.upScroll': 'Scroll up',
  'button.wrap': 'Wrap lines',
  'global.actions.copy': 'Copy',
  'global.actions.copy.success': 'Copied',
  'global.actions.export': 'Export',
  'global.actions.search': 'Search',
  'global.fetch.failed': 'Failed to load',
  'global.fullScreen': 'Fullscreen',
  'global.fullScreen.exit': 'Exit fullscreen',
  'global.message.deleteFailed': 'Delete failed',
  'global.noData': 'No data',
  'models.deploy.manifest.empty': 'No preview',
  'models.deploy.manifest.failed': 'Preview failed',
  'models.deploy.manifest.regenerate': 'Regenerate',
  'models.deploy.manifest.title': 'Launch YAML',
  'models.downloads.nodeSupervisor': 'Control node',
  'models.engines.emptyLocalOnlyHint':
    'Showing local images only. Click Refresh to pull remote tags / recommendations.',
  'models.instances.manifest.failed': 'Failed to load',
  'models.instances.manifest.pending':
    'Manifest is written after the container starts; it may be missing while deploying or after a failure.',
  'models.instances.modelName': 'Model',
  'models.instances.modelUid': 'Instance ID',
  'models.instances.replica': 'Replica',
  'models.instances.resources.gpuSlice': 'GPU slice',
  'models.instances.viewLogs.containerNoLines':
    'Container is registered but has no log lines yet. It will refresh shortly.',
  'models.repository.remoteSyncTimeout':
    'Sync timed out. Refresh the list later to confirm it finished.',
  'monitor.cluster.live': 'Live',
  'monitor.cluster.tabs': 'Cluster monitor tabs',
  'monitor.hami.policy.auto': 'Auto (cluster default; new deployments only)',
  'monitor.hami.policy.binpack': 'Binpack: pack jobs onto fewer GPUs',
  'monitor.hami.policy.spread': 'Spread: spread jobs across GPUs',
  'monitor.instances.active': 'In progress',
  'monitor.instances.bench.concurrency': 'Concurrency',
  'monitor.instances.bench.createdAt': 'Created',
  'monitor.instances.bench.createdNone':
    'Benchmark task was not created (instance must be READY; retry after init).',
  'monitor.instances.bench.prompts': 'Requests',
  'monitor.instances.bench.start': 'Start benchmark',
  'monitor.instances.bench.startMulti': 'Benchmark all instances',
  'monitor.instances.filter': 'Filter instance / model',
  'monitor.instances.gpuUtilRatio': 'GPU util ratio',
  'monitor.instances.logs': 'Logs',
  'monitor.instances.pending': 'Queued',
  'monitor.instances.query.title30d': 'Last 30 days · {uid}',
  'monitor.instances.resources': 'Instant load',
  'monitor.instances.toManage': 'Configure / scale',
  'monitor.instances.traces': 'Traces',
  'monitor.instances.usage.avgLatency': 'Avg latency',
  'monitor.instances.usage.fromTraffic': 'Same source as traffic monitor',
  'monitor.instances.usage.inputTokens': 'Input tokens',
  'monitor.instances.usage.outputTokens': 'Output tokens',
  'monitor.instances.usage.requests': 'Requests',
  'monitor.instances.usage.titleConv': 'Chat usage · last {days} days',
  'monitor.instances.usage.totalTokens': 'Total tokens',
  'monitor.traces.exportCsv': 'Export CSV',
  'monitor.traces.live': 'Live refresh',
  'monitor.traces.table.sourceIp': 'Source IP',
  'monitor.traces.table.status': 'Status',
  'monitor.traces.table.tokensPerSecond': 'Tokens / sec',
  'monitor.traces.table.ttft': 'TTFT',
  'pages.welcome.alertMessage': 'Welcome to PowerLLM',
};

function moduleForKey(key) {
  if (key.startsWith('monitor.')) return 'monitoring';
  if (key.startsWith('models.')) return 'models';
  if (key.startsWith('pages.')) return 'pages';
  return 'global';
}

function jsString(s) {
  return `'${String(s).replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
}

function appendKeys(filePath, entries) {
  if (!entries.length) return;
  let src = readFileSync(filePath, 'utf8');
  const idx = src.lastIndexOf('};');
  if (idx < 0) throw new Error(`no closing }; in ${filePath}`);
  const block = entries
    .map(([k, v]) => `  ${jsString(k)}: ${jsString(v)},\n`)
    .join('');
  writeFileSync(filePath, `${src.slice(0, idx)}\n${block}${src.slice(idx)}`);
}

function main() {
  const existing = collectLocaleKeyMap();
  for (const [locale, table] of [
    ['zh-CN', ZH],
    ['en-US', EN],
  ]) {
    const have = existing.get(locale).keys;
    const byFile = new Map();
    for (const [key, val] of Object.entries(table)) {
      if (have.has(key)) continue;
      const mod = moduleForKey(key);
      if (!byFile.has(mod)) byFile.set(mod, []);
      byFile.get(mod).push([key, val]);
    }
    for (const [mod, entries] of byFile) {
      appendKeys(join(localesRoot, locale, `${mod}.ts`), entries);
      console.log(`+ ${locale}/${mod}.ts  ${entries.length}`);
    }
  }
}

main();
