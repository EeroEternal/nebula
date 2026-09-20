import { GithubOutlined } from '@ant-design/icons';
import { DefaultFooter } from '@ant-design/pro-components';
import React from 'react';

const Footer: React.FC = () => {
  return (
    <DefaultFooter
      copyright={false}
      style={{
        background: 'none',
      }}
      links={[
        // {
        //   key: 'PowerLLM Enterprise',
        //   title: 'PowerLLM 企业版',
        //   href: '#',
        //   blankTarget: false,
        // }
      ]}
    />
  );
};

export default Footer;
