import { PROTABLE_OPTIONS_PUBLIC } from '@/constants';
import { l } from '@/utils/intl';
import { ProTable } from '@ant-design/pro-components';
import { Layout, theme } from 'antd';
import s from './index.module.css';
const { Content } = Layout;
const ReviewDetail: React.FC = () => {
  const {
    token: { colorBgContainer, borderRadiusLG },
  } = theme.useToken();
  const columns = [
    {
      title: 'Key',
      dataIndex: 'key',
      hideInSearch: true,
    },
    {
      title: '备注',
      dataIndex: 'note',
      hideInSearch: true,
    },

    {
      title: '状态',
      dataIndex: 'status',
      ellipsis: true,
      // hideInSearch: true,
    },
  ];

  // useEffect(() => {
  //   queryList(`${API_CONSTANTS.LLM_KEY_MANAGE}${theRequest.id}/`, {}, 'GET').then((res) => {
  //     setPageData(res.data)
  //   })

  // }, [])

  return (
    <Content
      style={{
        padding: 24,
        margin: 0,
        minHeight: 280,
        background: colorBgContainer,
        borderRadius: borderRadiusLG,
      }}
    >
      <div className={s.container}>
        <div className={s.title}>查询筛选</div>
        <div className={s.contentList}>
          <div className={s.item}>
            <span>模型ID：</span>
            <span>001</span>
          </div>
          <div className={s.item}>
            <span>模型名称：</span>
            <span>model 01</span>
          </div>
          <div className={s.item}>
            <span>{l('global.creator')}：</span>
            <span>admin</span>
          </div>
          <div className={s.item}>
            <span>{l('global.createTime')}：</span>
            <span>2024-03-28</span>
          </div>
          <div className={s.item}>
            <span>模型供应商：</span>
            <span>文心一言/阿里-通义千问</span>
          </div>
        </div>
      </div>
      <div className={s.container}>
        <div className={s.title}>AI KEY关联详情</div>
        <div className={s.contentList}>
          <ProTable
            {...PROTABLE_OPTIONS_PUBLIC}
            columns={columns}
            search={{ defaultCollapsed: false }}
          />
        </div>
      </div>
    </Content>
  );
};
export default ReviewDetail;
