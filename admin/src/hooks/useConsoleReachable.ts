import { useEffect, useState } from 'react';
import { probeConsoleReachable } from '@/utils/consoleApi';

/** 探活 Region Console。默认不可达，避免侧栏闪出后再打 404。 */
export function useConsoleReachable(): { reachable: boolean; probed: boolean } {
  const [reachable, setReachable] = useState(false);
  const [probed, setProbed] = useState(false);

  useEffect(() => {
    let mounted = true;
    probeConsoleReachable().then((ok) => {
      if (!mounted) {
        return;
      }
      setReachable(ok);
      setProbed(true);
    });
    return () => {
      mounted = false;
    };
  }, []);

  return { reachable, probed };
}
