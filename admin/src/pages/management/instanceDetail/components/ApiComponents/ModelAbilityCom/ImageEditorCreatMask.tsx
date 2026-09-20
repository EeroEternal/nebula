import { IconFont } from '@/components';
import { l } from '@/utils/intl';
import { DeleteOutlined, InboxOutlined, UndoOutlined } from '@ant-design/icons';
import type { UploadProps, ColorPickerProps, GetProp } from 'antd';
import { Button, Popover, Slider, Tooltip, Upload, ColorPicker, Image as AntdImage } from 'antd';
import type { RcFile } from 'antd/lib/upload';
import { MouseEvent, useEffect, useRef, useState, useMemo, FC, useCallback } from 'react';

type Color = Extract<GetProp<ColorPickerProps, 'value'>, string | { cleared?: boolean }>;
interface Point {
  x: number;
  y: number;
}

// 一条涂抹路径由多个坐标点构成 + 样式
interface Stroke {
  points: Point[];
  color: string;
  width: number;
}
interface FormValues {
  image?: RcFile;
  mask?: RcFile;
}
interface ImageEditorProps {
  value?: FormValues;
  onChange?: (v?: RcFile) => void;
  updateMask: (v?: RcFile) => void;
}

const ImageEditor: FC<ImageEditorProps> = ({ onChange, updateMask }) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const previewCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const previewCtxRef = useRef<CanvasRenderingContext2D | null>(null);
  const previewContainerRef = useRef<HTMLDivElement | null>(null);
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [originalImage, setOriginalImage] = useState<RcFile | undefined>(undefined);
  const [maskImage, setMaskImage] = useState<RcFile | undefined>(undefined);
  const [drawing, setDrawing] = useState<boolean>(false);
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  const [currentStroke, setCurrentStroke] = useState<Point[]>([]);
  const [lineWidth, setLineWidth] = useState<number>(35);
  const [lineColor, setLineColor] = useState<Color>('#000');

  const hexString = useMemo<string>(
    () => (typeof lineColor === 'string' ? lineColor : lineColor?.toHexString()),
    [lineColor],
  );
  const updateMaskForCanvasToFile = () => {
    if (previewCanvasRef.current) {
      previewCanvasRef.current.toBlob((blob) => {
        if (blob) {
          const file = new File([blob], `mask_${Date.now()}`, { type: 'image/png' }) as RcFile;
          updateMask(file);
        }
      }, 'image/png');
    }
  };
  const getPreviewCanvasScale = (img: HTMLImageElement | null) => {
    if (!previewContainerRef.current || !img) return 1;
    const previewContainerWidth = previewContainerRef.current.offsetWidth;
    const maxPreviewCanvasWidth = previewContainerWidth * 0.7; // 占容器的百分70%
    const maxPreviewCanvasHeight = 180; //这里如果变动，canvas的外层div的宽也要同步更改

    const previewWidthRatio = maxPreviewCanvasWidth / img.width;
    const previewHeightRatio = maxPreviewCanvasHeight / img.height;
    const previewScale = Math.min(1, previewWidthRatio, previewHeightRatio);
    return previewScale;
  };
  // 初始化当前蒙版
  const drawInitPreviewImage = useCallback(
    async (img: HTMLImageElement | null, isMask?: boolean) => {
      if (!previewCanvasRef.current || !previewContainerRef.current || !img) return;
      const previewCanvas = previewCanvasRef.current;
      const previewCtx = previewCanvas.getContext('2d');
      if (!previewCtx) return;
      const scale = getPreviewCanvasScale(img);
      previewCanvas.width = img.width;
      previewCanvas.height = img.height;
      previewCanvas.style.width = `${img.width * scale}px`;
      previewCanvas.style.height = `${img.height * scale}px`;
      if (isMask) {
        previewCtx.lineCap = 'round';
        previewCtx.lineJoin = 'round';
        previewCtx.drawImage(img, 0, 0);
      } else {
        previewCtx.fillStyle = '#000';
        previewCtx.fillRect(0, 0, img.width, img.height);
        updateMaskForCanvasToFile();
      }
      previewCtxRef.current = previewCtx;
    },
    [previewCanvasRef, previewContainerRef, originalImage],
  );
  const handleUpload: UploadProps['beforeUpload'] = (file) => {
    setOriginalImage(file);
    onChange?.(file);
    const reader = new FileReader();
    reader.onload = () => {
      const img = new Image();
      img.src = reader.result as string;
      img.onload = () => {
        setImage(img);
        drawInitPreviewImage(img);
      };
    };
    reader.readAsDataURL(file);
    return false;
  };
  const handleUploadMask: UploadProps['beforeUpload'] = (file) => {
    updateMask(file);
    setMaskImage(file);
    const reader = new FileReader();
    reader.onload = () => {
      const img = new Image();
      img.src = reader.result as string;
      img.onload = () => {
        drawInitPreviewImage(img, true);
      };
    };
    reader.readAsDataURL(file);
    return false;
  };
  const getCanvasScale = () => {
    if (!canvasRef.current) return 1;
    const styleWidth = parseFloat(canvasRef.current.style.width);
    return styleWidth ? styleWidth / canvasRef.current.width : 1;
  };

  // 鼠标按下开始涂抹
  const startDrawing = (e: MouseEvent<HTMLCanvasElement>) => {
    if (!image || !ctxRef.current) return;
    setDrawing(true);
    const rect = canvasRef.current!.getBoundingClientRect();
    const scale = getCanvasScale();

    const x = (e.clientX - rect.left) / scale;
    const y = (e.clientY - rect.top) / scale;
    const point: Point = { x, y };
    setCurrentStroke([point]);

    ctxRef.current.strokeStyle = hexString;
    ctxRef.current.lineWidth = lineWidth;
    ctxRef.current.beginPath();
    ctxRef.current.moveTo(x, y);
  };
  // 鼠标移动继续涂抹
  const draw = (e: MouseEvent<HTMLCanvasElement>) => {
    if (!drawing || !ctxRef.current) return;
    const rect = canvasRef.current!.getBoundingClientRect();
    const scale = getCanvasScale();
    const x = (e.clientX - rect.left) / scale;
    const y = (e.clientY - rect.top) / scale;
    ctxRef.current.lineTo(x, y);
    ctxRef.current.stroke();
    setCurrentStroke((prev) => [...prev, { x, y }]);
  };
  // 重新绘制所有内容（背景图 + 所有路径）
  const redrawCanvas = (
    strokesList: Stroke[],
    canvas: HTMLCanvasElement | null,
    ctx: CanvasRenderingContext2D | null,
    isPreview?: boolean,
  ) => {
    if (!image || !canvas || !ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (isPreview) {
      ctx.fillStyle = '#000';
      // 判断宽高不一致（原来的宽512的图片，canvas 也是512，这时又上传了一个mask（宽为1024）, canvas宽变为1024 这时如果继续涂抹，会导致比例不一致）
      if (
        (canvasRef.current?.width && canvasRef.current.width !== canvas.width) ||
        (canvasRef.current?.height && canvasRef.current.height !== canvas.height)
      ) {
        const scale = getPreviewCanvasScale(image);
        canvas.width = canvasRef.current.width;
        canvas.height = canvasRef.current.height;
        canvas.style.width = `${image.width * scale}px`;
        canvas.style.height = `${image.height * scale}px`;
      }
      ctx.fillRect(0, 0, canvas.width, canvas.height);
    } else {
      ctx.drawImage(image, 0, 0);
    }

    strokesList.forEach(({ points, color, width }) => {
      ctx.beginPath();
      ctx.strokeStyle = isPreview ? '#fff' : color;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      ctx.lineWidth = width;
      points.forEach((point, index) => {
        if (index === 0) ctx.moveTo(point.x, point.y);
        else ctx.lineTo(point.x, point.y);
      });
      ctx.stroke();
    });
    if (isPreview) {
      updateMaskForCanvasToFile();
    }
  };
  // 停止绘制，将当前笔画加入历史记录
  const stopDrawing = () => {
    if (!drawing) return;
    setDrawing(false);
    if (currentStroke.length > 0) {
      const newStroke: Stroke = {
        points: currentStroke,
        color: hexString,
        width: lineWidth,
      };
      const newStrokes = [...strokes, newStroke];
      setStrokes(newStrokes);
      redrawCanvas(newStrokes, canvasRef.current, ctxRef.current);
      redrawCanvas(newStrokes, previewCanvasRef.current, previewCtxRef.current, true);
    }
    setCurrentStroke([]);
  };

  // 撤销上一步操作
  const undo = () => {
    if (strokes.length === 0) return;
    const newStrokes = strokes.slice(0, -1);
    setStrokes(newStrokes);
    redrawCanvas(newStrokes, canvasRef.current, ctxRef.current);
    redrawCanvas(newStrokes, previewCanvasRef.current, previewCtxRef.current, true);
  };
  // 清除所有涂抹内容
  const clearStrokes = () => {
    setStrokes([]);
    redrawCanvas([], canvasRef.current, ctxRef.current);
    redrawCanvas([], previewCanvasRef.current, previewCtxRef.current, true);
  };
  // 重置为未上传状态
  const resetAll = () => {
    setImage(null);
    setOriginalImage(undefined);
    setStrokes([]);
    onChange?.(undefined);
    if (ctxRef.current && canvasRef.current) {
      ctxRef.current.clearRect(0, 0, canvasRef.current.width, canvasRef.current.height);
    }
    if (!maskImage && previewCanvasRef.current && previewCtxRef.current) {
      previewCtxRef.current.clearRect(
        0,
        0,
        previewCanvasRef.current.width,
        previewCanvasRef.current.height,
      );
      updateMask(undefined);
    }
  };
  const clearMask = () => {
    setMaskImage(undefined);

    if (previewCanvasRef.current && previewCtxRef.current) {
      if (image) {
        redrawCanvas(strokes, previewCanvasRef.current, previewCtxRef.current, true);
      } else {
        previewCtxRef.current.clearRect(
          0,
          0,
          previewCanvasRef.current.width,
          previewCanvasRef.current.height,
        );
        updateMask(undefined);
      }
    }
  };

  useEffect(() => {
    if (!image || !canvasRef.current || !containerRef.current) return;
    const canvas = canvasRef.current;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const containerWidth = containerRef.current.offsetWidth;
    const maxCanvasWidth = containerWidth * 0.7; // 占容器的百分70%
    const maxCanvasHeight = 230; //这里如果变动，canvas的外层div的宽也要同步更改

    const widthRatio = maxCanvasWidth / image.width;
    const heightRatio = maxCanvasHeight / image.height;
    const scale = Math.min(1, widthRatio, heightRatio);

    canvas.width = image.width;
    canvas.height = image.height;
    canvas.style.width = `${image.width * scale}px`;
    canvas.style.height = `${image.height * scale}px`;

    ctx.drawImage(image, 0, 0);
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.strokeStyle = hexString;
    ctx.lineWidth = lineWidth;
    ctxRef.current = ctx;
  }, [image]);

  return (
    <div>
      <div className="h-[250px]">
        {image ? (
          <div
            ref={containerRef}
            className="w-full h-full relative border border-dashed flex items-center justify-center rounded-[8px]"
          >
            <canvas
              ref={canvasRef}
              onMouseDown={startDrawing}
              onMouseMove={draw}
              onMouseUp={stopDrawing}
              onMouseLeave={stopDrawing}
              style={{ cursor: 'crosshair' }}
            />
            <div className="absolute flex flex-col gap-y-[4px] left-[10px]">
              <Tooltip title={l('global.actions.revoke')}>
                <Button icon={<UndoOutlined />} onClick={undo} size="small" />
              </Tooltip>
              <Tooltip title={l('global.actions.clear')}>
                <Button
                  icon={<IconFont name="icon-xiangpica1" />}
                  onClick={clearStrokes}
                  size="small"
                />
              </Tooltip>
              <Popover
                title={
                  <Slider
                    min={1}
                    max={50}
                    value={lineWidth}
                    onChange={(value: number) => {
                      setLineWidth(value);
                    }}
                    style={{ width: 150 }}
                  />
                }
              >
                <Button icon={<IconFont name="icon-tumo1" />} size="small" />
              </Popover>
              <ColorPicker size="small" value={lineColor} onChange={setLineColor} />
              <Tooltip title={l('global.actions.delete')}>
                <Button icon={<DeleteOutlined />} onClick={resetAll} size="small" />
              </Tooltip>
            </div>
          </div>
        ) : (
          <div className="h-full">
            <Upload.Dragger
              beforeUpload={handleUpload}
              showUploadList={false}
              customRequest={() => {}}
              accept="image/*"
            >
              <p className="ant-upload-drag-icon">
                <InboxOutlined />
              </p>
              <p className="ant-upload-text">{l('model.running.image.uploadInpaint')}</p>
              <p className="ant-upload-hint" style={{ fontSize: 14 }}>
                {l('model.running.image.uploadInpaintDesc')}
              </p>
            </Upload.Dragger>
          </div>
        )}
      </div>
      <div className="w-full h-[200px] flex items-center gap-x-[10px] mt-[10px]">
        <Upload.Dragger
          beforeUpload={handleUploadMask}
          showUploadList={false}
          customRequest={() => {}}
          accept="image/*"
          className="h-full w-1/2"
        >
          {maskImage ? (
            <>
              <div
                onClick={(e) => e.stopPropagation()}
                className="absolute top-[10px] right-[10px] w-[30px] h-[30px] flex items-center justify-center bg-[#fafafa] rounded-full hover:bg-primary hover:!text-[#fff]"
              >
                <DeleteOutlined onClick={clearMask} className="!text-[16px] !text-[#ccc]" />
              </div>
              <div onClick={(e) => e.stopPropagation()}>
                <AntdImage
                  className="object-contain"
                  width={150}
                  height={150}
                  src={URL.createObjectURL(maskImage)}
                />
              </div>
            </>
          ) : (
            <>
              <p className="ant-upload-drag-icon">
                <InboxOutlined />
              </p>
              <p className="ant-upload-text">{l('model.running.image.maskImage')}</p>
              <p className="ant-upload-hint">{l('model.running.image.maskImageDesc')}</p>
            </>
          )}
        </Upload.Dragger>
        <div
          ref={previewContainerRef}
          className="border border-[#d9d9d9] border-dashed rounded-[8px] h-full w-1/2 relative flex items-center justify-center bg-[#fafafa]"
        >
          <div className="absolute left-0 top-0 border-r border-b border-dashed rounded-br-[8px] py-[2px] px-[4px] bg-card z-[100]">
            {l('model.running.image.currentMaskImage')}
          </div>
          <canvas ref={previewCanvasRef} />
        </div>
      </div>
    </div>
  );
};
export default ImageEditor;
