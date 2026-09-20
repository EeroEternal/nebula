import { formatDisplayTime } from '@/utils';
export interface TableSearchParams {
  pageSize: number;
  current: number;
  [key: string]: unknown;
}

export function formPageParams(data: TableSearchParams){
  const {pageSize, current, ...params} = data;
  params.curPageNum = current;
  params.numPerPage = pageSize;
  return params
}

export function formatTime(data: number) {
  return formatDisplayTime(data * 1000);
}


export function formatToKUnits(num: number | undefined): string {
  if(num === undefined){
    return '0K'
  }
  // 向上取整，然后转换为字符串，并添加"k"单位
  return `${Math.ceil(num / 1024)}K`;
}