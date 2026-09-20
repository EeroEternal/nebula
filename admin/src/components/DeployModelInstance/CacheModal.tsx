import { App, Modal, Button } from 'antd';
import { UndoDot, Trash2, RotateCcw } from 'lucide-react';
import { FC, useState } from 'react';
import { ActionWithTips, IconButton } from '@/components';
import { formatDisplayTime } from '@/utils';
import { l, lGet } from '@/utils/intl';

export type DeployCacheItem = {
  model_uid: string;
  ctime: number;
};

interface CacheModalProps {
  historyData: DeployCacheItem[];
  currentModelUid?: string;
  onSelectCache: (historyDetail: DeployCacheItem) => void;
  onDeleteCache: (modelUid?: string) => void;
  onSelectDefault: () => void;
}
const CacheModal: FC<CacheModalProps> = ({
  historyData,
  currentModelUid,
  onSelectCache,
  onDeleteCache,
  onSelectDefault,
}) => {
  const { modal } = App.useApp();
  const [open, setOpen] = useState(false);
  const handleCancel = () => {
    setOpen(false);
  };
  const handleSelect = (historyDetail: DeployCacheItem) => {
    onSelectCache(historyDetail);
    setOpen(false);
  };
  const handleDelete = (modelUid?: string) => {
    modal.confirm({
      centered: true,
      title: lGet('models.deploy.cacheConfig.delete.tips'),
      onOk: () => onDeleteCache(modelUid),
    });
  };
  const handleDefault = () => {
    onSelectDefault();
    setOpen(false);
  };
  return (
    <>
      <ActionWithTips title={l('models.deploy.cacheConfig')}>
        <IconButton
          className="!w-8 !h-8 text-muted !rounded-[6px] hover:!bg-black-06 hover:!text-black"
          onClick={() => setOpen(true)}
        >
          <UndoDot size={18} />
        </IconButton>
      </ActionWithTips>
      <Modal
        centered
        open={open}
        title={l('models.deploy.cacheConfig')}
        onCancel={handleCancel}
        footer={
          <div className="flex justify-between">
            <div className="flex gap-2">
              <Button type="primary" icon={<RotateCcw size={14} />} onClick={handleDefault}>
                {l('models.deploy.cacheConfig.default')}
              </Button>
              {!!historyData.length && (
                <Button danger icon={<Trash2 size={14} />} onClick={() => handleDelete()}>
                  {l('global.actions.clearAll')}
                </Button>
              )}
            </div>

            <Button onClick={handleCancel}>{l('global.actions.close')}</Button>
          </div>
        }
      >
        <div className="space-y-3 pt-2 pb-1">
          {historyData.length ? (
            historyData.map((item) => (
              <div
                key={item.model_uid}
                className={`flex items-center justify-between gap-4 rounded-lg border p-4 cursor-pointer transition-colors ${
                  currentModelUid === item.model_uid
                    ? 'border-primary ring-1 ring-primary/30 bg-primary/5'
                    : 'border-border hover:border-primary/40'
                }`}
              >
                <div className="min-w-0 flex-1">
                  <div className="font-medium text-sm">{item.model_uid || '-'}</div>
                  <div className="text-xs text-muted mt-1 space-y-0.5">
                    <div>
                      {l('models.deploy.instanceName')}:&nbsp;{item.model_uid || '—'}
                    </div>
                    <div>
                      {l('global.lastUpdateTime')}:&nbsp;
                      {formatDisplayTime(item.ctime * 1000)}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  <Button type="primary" onClick={() => handleSelect(item)}>
                    {l('global.actions.use')}
                  </Button>
                  <Button danger onClick={() => handleDelete(item.model_uid)}>
                    {l('models.deploy.cacheConfig.delete')}
                  </Button>
                </div>
              </div>
            ))
          ) : (
            <div className="text-sm text-muted text-center py-12">
              {l('models.deploy.cacheConfig.empty', undefined, {
                value: l('global.actions.deploy'),
              })}
            </div>
          )}
        </div>
      </Modal>
    </>
  );
};

export default CacheModal;
