import { FC } from 'react';
import styles from './index.less';

interface WaveProgressBarProps {
  progress: number;
}
const WaveProgressBar: FC<WaveProgressBarProps> = ({ progress = 0 }) => {
  return (
    <div
      className={styles.indicator}
      style={
        {
          '--completion': `${progress}%`,
        } as React.CSSProperties
      }
    >
      <div className={styles.waterWrapper}>
        <div className={styles.water}></div>
      </div>
      <span className={styles.progress}>{progress}%</span>
    </div>
  );
};

export default WaveProgressBar;
