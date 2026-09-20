import { Dropdown, Button, Tag } from 'antd';
import type { MenuProps, DropDownProps } from 'antd';
import { GitCompare } from 'lucide-react';
import { useRequest } from 'ahooks';
import { useState, useCallback } from 'react';
import { useLocation, useModel } from '@umijs/max';
import { size } from 'lodash';
import cn from 'classnames';

import { l } from '@/utils/intl';
import request from '@/utils/request';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import { ModelAbility } from '@/constants/modelData';
import type { InstanceDetail } from '@/types/Public/data';
import { IntanceStatus } from '@/constants/intance';

const CompareBtn = () => {
  const { instanceDetail, compareData, updateState } = useModel('management.instanceDetail.model');
  const location = useLocation();
  const queryParams = new URLSearchParams(location.search);
  const modelId = queryParams.get('id');
  const showTags = size(compareData) > 1;
  const [menuItems, setMenuItems] = useState<MenuProps['items']>([]);
  const dropdownItemClick = (menuItem: InstanceDetail) => {
    updateState((prev) => {
      return {
        ...prev,
        compareData: [...prev.compareData, menuItem],
      };
    });
  };
  useRequest(
    () =>
      request<{ data: { results: InstanceDetail[] } }>('/models/instances', {
        params: ALL_LIST_PAGES_PARAMS,
      }),
    {
      onSuccess: (res) => {
        setMenuItems(
          (res?.data?.results || [])
            .filter(
              (item) =>
                (item.model_ability || []).includes(ModelAbility.ocr) &&
                item.status === IntanceStatus.READY,
            )
            .map((item) => ({
              label: item.model_uid,
              key: item.model_uid,
              onClick: () => dropdownItemClick(item),
              disabled: item.model_uid === instanceDetail.model_uid,
            })),
        );
      },
    },
  );

  const onOpenChangeDropdown: DropDownProps['onOpenChange'] = async (open) => {
    if (open) {
      setMenuItems(
        (menuItems || []).map((item) => {
          if (!item) return item;
          return {
            ...item,
            disabled: !!compareData.find((sub) => sub.model_uid === item.key),
          };
        }),
      );
    }
  };
  const handleCloseTag = useCallback(
    (id: string) => {
      updateState({ compareData: compareData.filter((item) => item.model_uid !== id) });
    },
    [compareData],
  );
  return (
    <Dropdown
      menu={{ items: menuItems }}
      placement="bottomRight"
      onOpenChange={onOpenChangeDropdown}
    >
      <div
        className={cn('flex flex-col items-end gap-2', {
          relative: showTags,
        })}
      >
        {showTags && (
          <div className="absolute -top-8 flex items-center gap-1">
            {compareData.map((item) => {
              if (item.model_uid === modelId) return null;
              return (
                <Tag
                  className="mr-0 bg-card"
                  key={item.model_uid}
                  closable
                  onClose={() => handleCloseTag(item.model_uid)}
                >
                  {item.model_uid}
                </Tag>
              );
            })}
          </div>
        )}
        <Button className="w-[100px]" size="large" type="primary" icon={<GitCompare size={14} />}>
          {l('models.instances.detail.compare')}
        </Button>
      </div>
    </Dropdown>
  );
};
export default CompareBtn;
