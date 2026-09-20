import { l, lGet } from '@/utils/intl';
import { App, Button, Dropdown } from 'antd';
import type { MenuProps } from 'antd';
import { Download, MoreHorizontal, Pin, Rocket, Trash2 } from 'lucide-react';
import { FC, useState } from 'react';
import { history } from '@umijs/max';
import { useRequest } from 'ahooks';
import { formatDisplayTime } from '@/utils';
import request from '@/utils/request';
import { IconButton, StatusTag } from '@/components';
import { ModelRepositoryListItem } from '@/types/Public/data';
import DownloadDrawer, { DownloadDraft } from '../DownloadDrawer';
import { repoPinKey } from '../../repoPin';

interface ModelListProps {
  modelType?: string;
  modelList: ModelRepositoryListItem[];
  pinnedKeys?: string[];
  onTogglePin?: (item: ModelRepositoryListItem) => void;
  deleteCustomModelCallBack: () => void;
}

const resolveItemType = (item: ModelRepositoryListItem, tabType?: string) =>
  tabType === 'custom' ? item.model_type || 'LLM' : tabType;

const ModelList: FC<ModelListProps> = ({
  modelType,
  modelList,
  pinnedKeys = [],
  onTogglePin,
  deleteCustomModelCallBack,
}) => {
  const { modal } = App.useApp();
  const isCustom = modelType === 'custom';
  const [downloadOpen, setDownloadOpen] = useState(false);
  const [downloadDraft, setDownloadDraft] = useState<DownloadDraft | null>(null);

  const { runAsync } = useRequest(
    ({ type, modelName }: { type: string; modelName: string }) =>
      request(`/model_registrations/${type}/${modelName}`, { method: 'delete' }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) deleteCustomModelCallBack();
      },
    },
  );

  /** 卡片点击进入模型详情（版本/下载），部署留给下载任务列表行 */
  const handleToDetail = (item: ModelRepositoryListItem) => {
    const type = resolveItemType(item, modelType);
    history.push(`/models/repository/${type}/${item.model_name}`);
  };

  const handleDeleteCustomModel = (item: ModelRepositoryListItem) => {
    const type = resolveItemType(item, modelType);
    modal.confirm({
      title: `${lGet('models.repository.modelDeleteTips')}${item.model_name}`,
      onOk: () => runAsync({ type: String(type), modelName: item.model_name }),
    });
  };

  const openDownload = (item: ModelRepositoryListItem) => {
    const type = resolveItemType(item, modelType);
    setDownloadDraft({
      modelName: item.model_name,
      modelType: String(type),
      modelData: item,
    });
    setDownloadOpen(true);
  };

  return (
    <>
      <div className="grid gap-3.5 md:grid-cols-2 lg:grid-cols-3 2xl:grid-cols-4">
        {modelList.map((item) => {
          const itemType = resolveItemType(item, modelType);
          const pinId = repoPinKey(String(itemType || 'LLM'), item.model_name);
          const pinned = pinnedKeys.includes(pinId);
          const cached =
            item.exist_cache ||
            item.cache_status ||
            (item?.model_specs || []).some((ele) => !!ele.cache_status);
          const updatedAt = item.updated_at
            ? formatDisplayTime(item.updated_at * 1000, 'YYYY-MM-DD HH:mm')
            : null;
          const moreMenu: MenuProps['items'] = [
            {
              key: 'delete',
              danger: true,
              icon: <Trash2 size={14} />,
              label: l('global.actions.delete'),
              onClick: () => handleDeleteCustomModel(item),
            },
          ];

          return (
            <RepoCard
              key={`${itemType}-${item.model_name}`}
              item={item}
              itemType={String(itemType || 'LLM')}
              pinned={pinned}
              cached={!!cached}
              updatedAt={updatedAt}
              isCustom={isCustom}
              moreMenu={moreMenu}
              onOpenDetail={() => handleToDetail(item)}
              onTogglePin={() => onTogglePin?.(item)}
              onDownload={() => openDownload(item)}
            />
          );
        })}
      </div>

      <DownloadDrawer
        open={downloadOpen}
        draft={downloadDraft}
        onClose={() => {
          setDownloadOpen(false);
          setDownloadDraft(null);
        }}
      />
    </>
  );
};

const RepoCard: FC<{
  item: ModelRepositoryListItem;
  itemType: string;
  pinned: boolean;
  cached: boolean;
  updatedAt: string | null;
  isCustom: boolean;
  moreMenu: MenuProps['items'];
  onOpenDetail: () => void;
  onTogglePin: () => void;
  onDownload: () => void;
}> = ({
  item,
  itemType,
  pinned,
  cached,
  updatedAt,
  isCustom,
  moreMenu,
  onOpenDetail,
  onTogglePin,
  onDownload,
}) => {
  const [hovered, setHovered] = useState(false);
  const showPin = pinned || hovered;
  return (
    <article
      className="relative rounded-lg bg-card text-default shadow-card border border-[color:var(--c-border-light)] hover:border-primary transition-colors group p-5 flex flex-col gap-3 min-h-[168px] focus-within:ring-2 focus-within:ring-primary/40 focus-within:ring-offset-1"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <button
        type="button"
        className="absolute inset-0 z-0 rounded-lg cursor-pointer bg-transparent border-0 p-0 focus:outline-none"
        onClick={onOpenDetail}
        aria-label={`${l('models.repository.openDetail')}: ${item.model_name}`}
      />

      <div className="relative z-10 flex items-start justify-between gap-2 pointer-events-none">
        <h3 className="tracking-tight text-[15px] font-bold line-clamp-2 text-default">
          {item.model_name}
        </h3>
        <div className="relative z-20 flex items-center gap-1 shrink-0 pointer-events-auto">
          <button
            type="button"
            className={`inline-flex h-8 w-8 items-center justify-center rounded-md border-0 text-muted hover:text-primary hover:bg-background-muted ${
              showPin ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'
            }`}
            aria-label={
              pinned
                ? l('models.repository.unpin')
                : l('models.repository.pin')
            }
            onClick={(e) => {
              e.stopPropagation();
              onTogglePin();
            }}
          >
            <Pin size={16} className={pinned ? 'fill-current text-primary' : ''} />
          </button>
          {isCustom && (
            <Dropdown menu={{ items: moreMenu }} trigger={['click']} placement="bottomRight">
              <IconButton
                className="!w-8 !h-8"
                aria-label={l('models.repository.moreActions')}
              >
                <MoreHorizontal size={16} />
              </IconButton>
            </Dropdown>
          )}
        </div>
      </div>

      <div className="relative z-10 flex flex-wrap items-center gap-2 pointer-events-none">
        <StatusTag tone={cached ? 'success' : 'neutral'}>
          {cached ? l('models.repository.cached') : l('models.repository.notCached')}
        </StatusTag>
      </div>

      <div className="relative z-10 flex flex-wrap items-center gap-2 text-xs text-muted pointer-events-none">
        {updatedAt && (
          <span>
            {l('models.repository.detail.updateTime')}: {updatedAt}
          </span>
        )}
      </div>

      <div className="relative z-10 mt-auto flex items-center justify-end gap-2 pointer-events-auto pt-1">
        <Button
          size="small"
          icon={<Download size={14} />}
          className="!border-[color:var(--c-border)]"
          aria-label={`${l('models.repository.download.action')} ${item.model_name}`}
          onClick={(e) => {
            e.stopPropagation();
            onDownload();
          }}
        >
          {l('models.repository.download.action')}
        </Button>
        <Button
          type="primary"
          size="small"
          icon={<Rocket size={14} />}
          aria-label={`${l('global.actions.deploy')} ${item.model_name}`}
          onClick={(e) => {
            e.stopPropagation();
            history.push(`/models/repository/${itemType}/${item.model_name}?activeTab=deploy`);
          }}
        >
          {l('global.actions.deploy')}
        </Button>
      </div>
    </article>
  );
};
export default ModelList;
