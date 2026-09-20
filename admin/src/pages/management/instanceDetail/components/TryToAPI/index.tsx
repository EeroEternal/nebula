import { Button, Drawer, Segmented } from 'antd';
import { CodeXml, ExternalLink, Copy } from 'lucide-react';
import React, { useMemo, useState } from 'react';
import { useModel } from '@umijs/max';
import { IconButton } from '@/components';
import { copyToClipboard, highlightJson } from '@/utils';
import { ModelAbility } from '@/constants/modelData';
import modelAbilityConfig, { chatCodeExample } from '../ModelAbilityCom/modelAbilityConfig';
import {
  CodeType,
  tabsForApi,
  generatePython,
  generateTS,
  generateJava,
  generateGo,
  generateShell,
} from '../../utils';
import type { ApiSchema } from '../../utils';

type Props = {
  /** Chat footer uses small; ability forms keep large */
  size?: 'small' | 'middle' | 'large';
};

const TryToAPI: React.FC<Props> = ({ size = 'large' }) => {
  const { instanceDetail, selectModelAbility, replicaId } = useModel(
    'management.instanceDetail.model',
  );
  const [open, setOpen] = useState(false);
  const [activeTab, setActiveTab] = useState(CodeType.Python);
  const fill = useMemo(
    () => ({
      modelUid: instanceDetail?.model_uid || '',
      replicaId: replicaId || '',
    }),
    [instanceDetail?.model_uid, replicaId],
  );
  const code = useMemo(() => {
    const hasChat = instanceDetail.model_ability.includes(ModelAbility.chat);
    const codeExample = (
      hasChat
        ? chatCodeExample
        : modelAbilityConfig[selectModelAbility]
        ? {
            ...modelAbilityConfig[selectModelAbility].codeExample,
            name: selectModelAbility,
            path: modelAbilityConfig[selectModelAbility].requestApi,
          }
        : undefined
    ) as ApiSchema;

    if (!codeExample) return '';
    switch (activeTab) {
      case CodeType.Python:
        return generatePython(codeExample, fill);
      case CodeType.TypeScript:
        return generateTS(codeExample, fill);
      case CodeType.Java:
        return generateJava(codeExample, fill);
      case CodeType.Go:
        return generateGo(codeExample, fill);
      case CodeType.Shell:
        return generateShell(codeExample, fill);
      default:
        return '';
    }
  }, [instanceDetail, selectModelAbility, activeTab, fill]);
  const showDrawer = () => setOpen(true);
  const closeDrawer = () => setOpen(false);
  const handleTabChange = (tab: string) => setActiveTab(tab as CodeType);
  const handleGetAPIKey = () => {
    window.open(`${window.PREFIX_PATH ? `/${window.PREFIX_PATH}` : ''}/admin/secretKey`);
  };
  const handleCopy = (code: string) => {
    const replacements: Record<string, string> = {
      '{MODEL_UID}': fill.modelUid,
      '{REPLICA_ID}': fill.replicaId,
    };
    const result = code.replace(/\{API_KEY\}|\{MODEL_UID\}|\{REPLICA_ID\}/g, (match) => {
      return replacements[match] ?? match;
    });
    copyToClipboard(result);
  };

  return (
    <>
      <Button
        type={size === 'small' ? 'default' : 'primary'}
        size={size}
        icon={<CodeXml size={14} />}
        onClick={showDrawer}
        className={
          size === 'small'
            ? 'bg-white border-[color:var(--c-border-light)] text-default hover:!bg-white hover:!border-[color:var(--c-border-light)] hover:!text-default'
            : undefined
        }
      >
        Try To API
      </Button>
      <Drawer
        open={open}
        title="Try To API"
        width="50%"
        onClose={closeDrawer}
        extra={
          <Button icon={<ExternalLink size={14} />} onClick={handleGetAPIKey}>
            Get API Key
          </Button>
        }
        className="re"
      >
        <Segmented
          size="large"
          block
          value={activeTab}
          options={tabsForApi}
          onChange={handleTabChange}
        />

        <div className="relative">
          <IconButton
            className="absolute right-3 top-3 !w-7 !h-7 hover:text-primary hover:bg-primary/15"
            onClick={() => handleCopy(code)}
          >
            <Copy size={14} />
          </IconButton>
          <pre
            className="mt-3 rounded-lg p-2 text-xs font-mono bg-background-muted/50 whitespace-pre-wrap break-all"
            dangerouslySetInnerHTML={{
              __html: highlightJson(code),
            }}
          />
        </div>
      </Drawer>
    </>
  );
};
export default TryToAPI;
