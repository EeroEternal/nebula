import { l } from '@/utils/intl';
import { LogoutOutlined, UnlockOutlined } from '@ant-design/icons';
import { history, useModel } from '@umijs/max';
import { Spin } from 'antd';
import { createStyles } from 'antd-style';
import type { MenuInfo } from 'rc-menu/lib/interface';
import React, { useCallback, useState } from 'react';
import { flushSync } from 'react-dom';
import { deleteLocal, getLocal } from '@/utils';
import type { ModelInitialState } from '@/types/global';
import { LOGIN_PATH } from '@/constants';
import HeaderDropdown from '../HeaderDropdown';
import UpdatePasswordModal from './UpdatePasswordModal';
import request from '@/utils/request';

export type GlobalHeaderRightProps = {
  menu?: boolean;
  children?: React.ReactNode;
};

export const AvatarName = () => {
  const { initialState } = useModel('@@initialState');
  const { currentUser } = initialState || {};
  return <span className="anticon">{currentUser?.name}</span>;
};

const useStyles = createStyles(({ token }) => {
  return {
    action: {
      display: 'flex',
      height: '48px',
      marginLeft: 'auto',
      overflow: 'hidden',
      alignItems: 'center',
      padding: '0 8px',
      cursor: 'pointer',
      borderRadius: token.borderRadius,
      '&:hover': {
        backgroundColor: token.colorBgTextHover,
      },
    },
  };
});

export const AvatarDropdown: React.FC<GlobalHeaderRightProps> = ({ children }) => {
  const { initialState, setInitialState } = useModel('@@initialState');
  const [showUpdatePasswordModal, setShowUpdatePasswordModal] = useState(false);

  const loginOut = async () => {
    const { token, token_type } = getLocal('user') || {};
    deleteLocal('user');
    history.replace(LOGIN_PATH);
    setInitialState(
      (s) => ({ ...s, currentUser: undefined, globalConfig: {} } as ModelInitialState),
    );
    await request('/user/logout', { method: 'post', Authorization: `${token_type} ${token}` });
  };
  const { styles } = useStyles();

  const onMenuClick = useCallback(
    (event: MenuInfo) => {
      const { key } = event;
      if (key === 'logout') {
        flushSync(() => {
          setInitialState((s) => ({ ...s, currentUser: undefined } as ModelInitialState));
        });
        loginOut();
        return;
      } else if (key === 'updatepassword') {
        setShowUpdatePasswordModal(true);
        return;
      }
      history.push(`/account/${key}`);
    },
    [setInitialState],
  );

  const loading = (
    <span className={styles.action}>
      <Spin
        size="small"
        style={{
          marginLeft: 8,
          marginRight: 8,
        }}
      />
    </span>
  );

  if (!initialState) {
    return loading;
  }

  const { currentUser } = initialState;

  if (!currentUser || !currentUser.name) {
    return loading;
  }

  const menuItems = [
    {
      key: 'updatepassword',
      icon: <UnlockOutlined />,
      label: l('global.login.changePassword'),
    },
    {
      type: 'divider' as const,
    },
    {
      key: 'logout',
      icon: <LogoutOutlined />,
      label: l('global.login.exit'),
    },
  ];

  return (
    <>
      <HeaderDropdown
        menu={{
          selectedKeys: [],
          onClick: onMenuClick,
          items: menuItems,
        }}
      >
        {children}
      </HeaderDropdown>

      {showUpdatePasswordModal && (
        <UpdatePasswordModal
          visible={showUpdatePasswordModal}
          handleCancel={() => setShowUpdatePasswordModal(false)}
        />
      )}
    </>
  );
};
