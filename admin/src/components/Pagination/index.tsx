import { Button } from 'antd';
import { FC } from 'react';

import { l } from '@/utils/intl';

interface PaginationProps {
  page: number;
  pageSize?: number;
  total: number;
  onPageChange: (page: number) => void;
}
const Pagination: FC<PaginationProps> = ({ page = 1, pageSize = 10, total = 0, onPageChange }) => {
  return (
    <div className="flex justify-center gap-2">
      <Button disabled={page === 1} onClick={() => onPageChange(page - 1)}>
        {l('global.actions.prePage')}
      </Button>
      <span className="flex items-center px-3 text-sm">
        {page} / {Math.ceil(total / pageSize)}
      </span>
      <Button disabled={page >= Math.ceil(total / pageSize)} onClick={() => onPageChange(page + 1)}>
        {l('global.actions.nextPage')}
      </Button>
    </div>
  );
};
export default Pagination;
