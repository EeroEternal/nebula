import React, { useEffect, useState } from 'react';
import { Badge, Select } from 'antd';
import { GlobalOutlined } from '@ant-design/icons';

import {
  fetchRegionStatus,
  getCurrentRegion,
  isRegionModeEnabled,
  setCurrentRegion,
  setRegionModeEnabled,
  type RegionInfo,
} from '@/utils/region';

/**
 * 顶部 region 切换器。
 * - 未接入 Console 或只有一个 region 时不渲染（界面与单站点现状一致）；
 * - 首次发现多 region 或切换 region 时刷新页面，使 DOMAIN_API 前缀生效。
 */
const RegionSwitcher: React.FC = () => {
  const [regions, setRegions] = useState<RegionInfo[]>([]);

  useEffect(() => {
    let mounted = true;
    fetchRegionStatus().then((list) => {
      if (!mounted) {
        return;
      }
      if (!list || list.length < 2) {
        // 单 region / 未接入 Console：退化为现状
        if (isRegionModeEnabled()) {
          setRegionModeEnabled(false);
          setCurrentRegion('');
          window.location.reload();
        }
        return;
      }
      setRegions(list);
      if (!isRegionModeEnabled() || !list.some((r) => r.name === getCurrentRegion())) {
        setRegionModeEnabled(true);
        setCurrentRegion(list[0].name);
        window.location.reload();
      }
    });
    return () => {
      mounted = false;
    };
  }, []);

  if (regions.length < 2) {
    return null;
  }

  const onChange = (region: string) => {
    setCurrentRegion(region);
    window.location.reload();
  };

  return (
    <Select
      size="small"
      className="min-w-[140px]"
      value={getCurrentRegion()}
      onChange={onChange}
      suffixIcon={<GlobalOutlined />}
      options={regions.map((r) => ({
        value: r.name,
        label: (
          <span>
            <Badge
              status={r.health?.status === 'online' ? 'success' : 'error'}
              className="mr-[4px]"
            />
            {r.display_name || r.name}
          </span>
        ),
      }))}
    />
  );
};

export default RegionSwitcher;
