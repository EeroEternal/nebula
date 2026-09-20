import { useState, FC, useMemo } from 'react';
import {
  ProFormSelect,
  ProFormText,
  // ProFormSwitch,
} from '@ant-design/pro-components';
import type { FormInstance } from 'antd';

import { l, lGet } from '@/utils/intl';
import { DeviceInfo } from '@/types/Public/data';
import ProCardForConfig from './ProCardForConfig';

interface MindIEParamsProps {
  devices: DeviceInfo[];
  form: FormInstance;
}
const MindIEParams: FC<MindIEParamsProps> = ({ devices }) => {
  const [baseConfigOpen, setBaseConfigOpen] = useState(true);
  const ipOptions = useMemo(
    () => ['127.0.0.1'].concat((devices || []).map((item) => item.worker_address.split(':')?.[0])),
    [devices],
  );
  const positiveIntegerRule = [
    { pattern: /^[1-9]\d*$/, message: l('models.deploy.MindIE.positiveIntegerRule') },
  ];
  const baseFormConfig = useMemo(
    () => [
      {
        fieldKey: 'ip_address',
        label: lGet('models.deploy.MindIE.ip'),
        placeholder: ipOptions[0],
        rules: [{ required: true, message: lGet('models.deploy.MindIE.ip.rule') }],
        options: ipOptions,
      },
      {
        fieldKey: 'port',
        label: lGet('models.deploy.MindIE.port'),
        placeholder: '1025',
        rules: positiveIntegerRule,
      },
      {
        fieldKey: 'model_total_length',
        label: lGet('models.deploy.MindIE.contextLength'),
        placeholder: '2560',
        rules: positiveIntegerRule,
      },
      {
        fieldKey: 'npuMemSize',
        label: lGet('models.deploy.MindIE.gpuMemory'),
        placeholder: '-1',
        rules: [
          { pattern: /^-1$|^[1-9]\d*$/, message: lGet('models.deploy.MindIE.gpuMemoryRule') },
        ],
      },
      {
        fieldKey: 'model_input_length',
        label: lGet('models.deploy.MindIE.modelInputLength'),
        placeholder: '2048',
        rules: positiveIntegerRule,
      },
      {
        fieldKey: 'model_output_length',
        label: lGet('models.deploy.MindIE.modelOutputLength'),
        placeholder: '512',
        rules: positiveIntegerRule,
      },
    ],
    [ipOptions],
  );
  // const advancedFormConfig = [
  //   { fieldKey: 'splitefuse', label: 'Splitefuse' },
  //   { fieldKey: 'prefixcache', label: 'Prefixcache' },
  //   { fieldKey: 'supprot_select_batch', label: 'SupportSelectBatch' },
  //   {
  //     fieldKey: 'multi_machine_inference',
  //     label: l('models.deploy.MindIE.multiMachineInference'),
  //   },
  // ] as const;
  // const handleAdvancedSwitchChange = (
  //   switched: boolean,
  //   fieldKey: (typeof advancedFormConfig)[number]['fieldKey'],
  // ) => {
  //   if (switched) {
  //     advancedFormConfig
  //       .filter((item) => item.fieldKey !== fieldKey)
  //       .forEach((item) => {
  //         form.setFieldValue(['kwargs', 'advanced_config', 0, item.fieldKey], false);
  //       });
  //   }
  // };

  // {
  //   ...baseCollapsePanel,
  //   key: 'mindie_advanced_config',
  //   label: l('models.deploy.MindIE.advancedConfig'),
  //   children: (
  //     <ProFormGroup>
  //       {advancedFormConfig.map((item) => (
  //         <ProFormSwitch
  //           {...item}
  //           name={['kwargs', 'advanced_config', 0, item.key]}
  //           colProps={{ span: 12 }}
  //           fieldProps={{
  //             onChange: (switched) => handleAdvancedSwitchChange(switched, item.key),
  //           }}
  //         />
  //       ))}
  //     </ProFormGroup>
  //   ),
  // },

  const handleAdvancedConfig = (collapsed: boolean) => {
    setBaseConfigOpen(collapsed);
  };
  return (
    <ProCardForConfig
      collapsed={baseConfigOpen}
      onCollapse={handleAdvancedConfig}
      title={l('models.deploy.MindIE.baseconfig')}
    >
      <div className="grid grid-cols-2">
        {baseFormConfig.map((item) =>
          item.fieldKey === 'ip_address' ? (
            <ProFormSelect
              {...item}
              key={item.fieldKey}
              name={['kwargs', 'mindIE_config', 0, item.fieldKey]}
            />
          ) : (
            <ProFormText
              {...item}
              key={item.fieldKey}
              name={['kwargs', 'mindIE_config', 0, item.fieldKey]}
            />
          ),
        )}
      </div>
    </ProCardForConfig>
  );
};
export default MindIEParams;
