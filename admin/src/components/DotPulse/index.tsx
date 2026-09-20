import { FC, PropsWithChildren } from 'react';
import styles from './index.less';

const DotPulse: FC<PropsWithChildren> = ({ children }) => {
  return (
    <div className="flex items-center my-[4px] gap-x-[4px]">
      {children}
      <div className={styles['dot-pulse']}>
        <span />
        <span />
        <span />
      </div>
    </div>
  );
};
export default DotPulse;
