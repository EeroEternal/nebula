import { useEffect, useState } from 'react';

/** 监测「是否距离底部超过 N px」 */
export default function useScrollBottomDetection(
  containerRef: React.RefObject<HTMLElement>,
  threshold: number = 100 // 距离底部多少 px 显示按钮
) {
  const [isFarFromBottom, setIsFarFromBottom] = useState(false);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = el;
      const distanceToBottom = scrollHeight - scrollTop - clientHeight;
      setIsFarFromBottom(distanceToBottom > threshold);
    };

    el.addEventListener('scroll', handleScroll);
    handleScroll(); // 初始触发一次

    return () => {
      el.removeEventListener('scroll', handleScroll);
    };
  }, [containerRef, threshold]);

  return isFarFromBottom;
}