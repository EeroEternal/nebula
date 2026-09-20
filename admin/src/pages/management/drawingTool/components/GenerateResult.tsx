import { Progress, Watermark, Button, Checkbox } from 'antd';
import { size } from 'lodash';
import { FC, useCallback, useEffect } from 'react';
import { DrawerAttachment } from '@/components';
import { transformRate } from '@/utils';
import { useSetState } from 'ahooks';
import { l } from '@/utils/intl';
import { serializeFieldsToPrompt } from '../utils';
interface GenerateResultProps {
  loading: boolean;
  progress: number;
  result: {
    images: string[];
    info: Record<string, unknown>;
    parameters: Record<string, unknown>;
  };
}
interface GenerateResultState {
  startDownLoad: boolean;
  downLoading: boolean;
  downLoadList: string[];
}
const GenerateResult: FC<GenerateResultProps> = ({ loading, progress, result }) => {
  const { images = [], info } = result || {};
  const imageList: string[] = (images || []).map(
    (base64: string) => `data:image/png;base64,${base64}`,
  );
  const [{ startDownLoad, downLoading, downLoadList }, setState] = useSetState<GenerateResultState>(
    {
      startDownLoad: false,
      downLoading: false,
      downLoadList: [],
    },
  );
  const isDownLoadAll = size(imageList) === size(downLoadList);
  const handleStartDownLoad = useCallback(() => setState({ startDownLoad: true }), []);
  const handleCancelDownLoad = useCallback(
    () => setState({ startDownLoad: false, downLoadList: [] }),
    [],
  );
  const handleSelectImg = useCallback(
    (value: string[]) => setState({ downLoadList: value as string[] }),
    [],
  );
  const handleSelectAll = useCallback(() => {
    setState({ downLoadList: isDownLoadAll ? [] : imageList });
  }, [imageList, isDownLoadAll]);
  const handleSaveToLocal = useCallback(() => {
    if (!size(downLoadList)) return;
    setState({ downLoading: true });
    downLoadList.forEach((item, i) => {
      const a = document.createElement('a');
      a.href = item;
      a.download = `image_${i + 1}`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
    });
    setState({ downLoading: false });
  }, []);
  useEffect(() => {
    setState({
      startDownLoad: false,
      downLoading: false,
      downLoadList: [],
    });
  }, [loading]);
  if (loading) {
    return (
      <div className="w-1/2 bg-[#f0f1fa] rounded-[8px] flex justify-center h-[600px]">
        <div className="w-[60%] h-full flex flex-col items-center justify-center">
          <img src="/drawing.gif" alt="" />
          <Progress percent={transformRate(progress)} strokeColor={window.THEME_PRIMARY_COLOR} />
          <div>正在生成中，请稍后...</div>
        </div>
      </div>
    );
  }
  if (size(imageList)) {
    return (
      <div className="w-1/2 bg-[#f0f1fa] rounded-[8px] p-[10px] h-auto min-h-[600px] flex-1 flex flex-col">
        <Watermark
          className=" bg-card rounded-[8px] p-[20px] h-[450px] !overflow-y-auto"
          content="PowerLLM"
        >
          {startDownLoad ? (
            <Checkbox.Group
              className="grid grid-cols-4 gap-[20px] checkbox-group-img"
              value={downLoadList}
              onChange={handleSelectImg}
            >
              {imageList.map((item) => (
                <Checkbox value={item} key={item}>
                  <DrawerAttachment
                    type="image"
                    url={item}
                    width={'100%'}
                    height={150}
                    preview={false}
                  />
                </Checkbox>
              ))}
            </Checkbox.Group>
          ) : (
            <div className="grid grid-cols-4 gap-[20px] ">
              {imageList.map((url) => (
                <DrawerAttachment
                  key={url}
                  type="image"
                  url={url}
                  urlList={imageList}
                  width={'100%'}
                  height={150}
                />
              ))}
            </div>
          )}
        </Watermark>
        <div className="mt-[20px]">
          <div className="flex justify-center items-center gap-x-[10px]">
            {startDownLoad ? (
              <>
                <span>
                  {l('management.drawingTool.download.selectNum', undefined, {
                    num: (
                      <span className="text-primary">{`${size(downLoadList)}/${size(
                        imageList,
                      )}`}</span>
                    ),
                  })}
                </span>
                <Button type="primary" onClick={handleSelectAll}>
                  {isDownLoadAll ? l('global.actions.deselectAll') : l('global.actions.selectAll')}
                </Button>
                <Button type="primary" onClick={handleSaveToLocal} loading={downLoading}>
                  {l('global.actions.saveToLocal')}
                </Button>
                <Button onClick={handleCancelDownLoad}>{l('global.actions.cancel')}</Button>
              </>
            ) : (
              <Button type="primary" onClick={handleStartDownLoad} className="w-[100px]">
                {l('global.actions.downLoad')}
              </Button>
            )}
          </div>
          <pre className='whitespace-pre-wrap text-[#666]'>{serializeFieldsToPrompt(info)}</pre>
        </div>
      </div>
    );
  }

  return (
    <div className="w-1/2 bg-[#f0f1fa] rounded-[8px] flex flex-col items-center justify-center h-[600px]">
      <img src="/drawingImg.png" alt="" className="w-[100px]" />
      <div className="text-[18px]">✨ 在线绘图 ✨</div>
    </div>
  );
};
export default GenerateResult;
