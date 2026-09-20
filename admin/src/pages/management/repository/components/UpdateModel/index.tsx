import { Badge, Button, App, Modal, Progress, Tooltip } from 'antd';
import { FC, useState } from 'react';
import { clearCache, useRequest } from 'ahooks';
import { useModel, useParams } from '@umijs/max';
import { CloudDownload, Loader2, Settings2 } from 'lucide-react';

import { UPDATE_MODEL_TYPE } from '@/constants/repository';
import { SETTING_MODAL_TABS } from '@/constants';
import { readThrownMessage, thrownName } from '@/utils/formatApiError';
import request from '@/utils/request';
import { l, lGet } from '@/utils/intl';
import { ModelInitialState } from '@/types/global';

interface UpdateModelProps {
  modelHubApiToken?: string;
  submitCallBack: (type: string) => void;
}

/** 模型中心配置 + 获取远程（对齐引擎页：进度可关、同步中再点可重开） */
const UpdateModel: FC<UpdateModelProps> = ({ modelHubApiToken, submitCallBack }) => {
  const { message } = App.useApp();
  const { setInitialState } = useModel('@@initialState');
  const params = useParams();
  const routeType = params?.modelType || 'LLM';
  const allowed = UPDATE_MODEL_TYPE.map((i) => i.value);
  const syncType = allowed.includes(routeType) ? routeType : 'LLM';
  /** 配置保存后即时刷新；避免父页 60s 缓存导致误判无密钥 */
  const [liveToken, setLiveToken] = useState<string | undefined>();
  const hasToken = !!(liveToken ?? modelHubApiToken);
  /** 进度弹窗展示；关闭不中断请求 */
  const [showProgress, setShowProgress] = useState(false);

  const showModelHubModal = () => {
    setInitialState(
      (prev) =>
        ({
          ...prev,
          settingModalActiveTab: SETTING_MODAL_TABS.MODEL_HUB,
          settingModalVisible: true,
        } as ModelInitialState),
    );
  };

  const { run, loading } = useRequest(
    () =>
      request('/models/update_type', {
        method: 'post',
        data: { model_type: syncType },
        // Hub LLM 全量目录可能较大；超时后结束 loading，避免弹窗一直转
        timeout: 600_000,
      }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          message.success(
            lGet('models.repository.remoteSyncDone'),
          );
          submitCallBack(syncType);
          return;
        }
        // 10000 可能是缺密钥，也可能是 Hub 请求失败；已有 token 时勿再弹配置
        if (res?.data?.code === 10000 && !hasToken) {
          showModelHubModal();
          return;
        }
        message.error(
          res?.data?.message ||
            String(
              lGet('models.repository.remoteSyncFailed'),
            ),
        );
      },
      onError: (e: unknown) => {
        const timedOut =
          thrownName(e) === 'TimeoutError' ||
          /timeout|timed out|ECONNABORTED/i.test(readThrownMessage(e) || String(e));
        message.error(
          timedOut
            ? String(
                lGet('models.repository.remoteSyncTimeout',
                ),
              )
            : readThrownMessage(e) ||
                String(
                  lGet('models.repository.remoteSyncFailed'),
                ),
        );
      },
      onFinally: () => {
        setShowProgress(false);
      },
    },
  );

  const { loading: probing, run: probeAndSync } = useRequest(
    async () => {
      clearCache('setting-model-hub');
      const res = await request('/setting/model_hub');
      return res?.data?.data?.api_token as string | undefined;
    },
    {
      manual: true,
      onSuccess: (token) => {
        setLiveToken(token || '');
        if (token) {
          setShowProgress(true);
          run();
          return;
        }
        message.warning(lGet('models.repository.beforeUpdateTips'));
        showModelHubModal();
      },
    },
  );

  const syncing = loading || probing;

  const handleRemoteSync = () => {
    // 同步中再次点击：只重新打开进度，不重复发起
    if (syncing) {
      setShowProgress(true);
      return;
    }
    probeAndSync();
  };

  const dismissProgress = () => setShowProgress(false);

  const typeLabel = String(lGet(`global.model.type.${syncType}`, syncType));

  return (
    <div className="flex items-center gap-2 shrink-0">
      <Button icon={<Settings2 size={16} />} onClick={showModelHubModal}>
        {l('app.settings.modal.modelHub')}
      </Button>
      <Tooltip
        title={
          syncing
            ? lGet('models.engines.remoteSyncProgressReopenHint',
              )
            : lGet('models.repository.remoteSyncHint',
              )
        }
      >
        <Badge
          dot={!hasToken}
          styles={{ indicator: { width: 8, height: 8, zIndex: 100 } }}
        >
          {/* 不用 ant loading：会吞 onClick，同步中无法再次打开进度 */}
          <Button
            icon={
              syncing ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <CloudDownload size={16} />
              )
            }
            onClick={handleRemoteSync}
          >
            {l('models.engines.remoteSync')}
          </Button>
        </Badge>
      </Tooltip>
      <Modal
        open={showProgress && syncing}
        title={l('models.engines.remoteSync')}
        closable
        maskClosable
        keyboard
        destroyOnClose
        onCancel={dismissProgress}
        centered
        width={420}
        footer={
          <div className="flex justify-end">
            <Button onClick={dismissProgress}>
              {l('models.engines.remoteSyncProgressDismiss')}
            </Button>
          </div>
        }
      >
        <div className="flex flex-col gap-3 py-1">
          <p className="text-[12px] text-muted m-0 leading-relaxed">
            {l('models.engines.remoteSyncProgressLeaveHint',
            )}
          </p>
          <p className="text-sm text-secondary m-0">
            {lGet('models.repository.remoteSyncProgressHint',
              { type: typeLabel },
            )}
          </p>
          <Progress
            percent={probing ? 15 : 70}
            status="active"
            showInfo={false}
          />
        </div>
      </Modal>
    </div>
  );
};
export default UpdateModel;
