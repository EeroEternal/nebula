import { IconFont } from '@/components';
import { l } from '@/utils/intl';
import { DeleteOutlined, InboxOutlined, UndoOutlined } from '@ant-design/icons';
import type { FormInstance, UploadProps, ColorPickerProps, GetProp } from 'antd';
import { Button, Popover, Slider, Tooltip, Upload, ColorPicker } from 'antd';
import type { RcFile } from 'antd/lib/upload';
import { forwardRef, MouseEvent, useEffect, useRef, useState, useMemo } from 'react';

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

interface ImageEditorProps {
  value?: string;
  onChange?: (base64?: string) => void;
}

const ImageEditor = forwardRef<FormInstance, ImageEditorProps>(({ value, onChange }, ref) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [drawing, setDrawing] = useState<boolean>(false);
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  const [currentStroke, setCurrentStroke] = useState<Point[]>([]);
  const [lineWidth, setLineWidth] = useState<number>(10);
  const [lineColor, setLineColor] = useState<Color>('#000');

  const hexString = useMemo<string>(
    () => (typeof lineColor === 'string' ? lineColor : lineColor?.toHexString()),
    [lineColor],
  );

  // 上传图片后初始化 canvas 尺寸及绘图上下文
  useEffect(() => {
    if (!image || !canvasRef.current || !containerRef.current) return;
    const canvas = canvasRef.current;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const containerWidth = containerRef.current.offsetWidth;
    const maxCanvasWidth = containerWidth * 0.7; // 占容器的百分70%
    const maxCanvasHeight = 180; //这里如果变动，canvas的外层div的宽也要同步更改

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

    notifyChange();
  }, [image]);

  // 每次涂抹更新后同步 base64
  useEffect(() => {
    if (image) notifyChange();
  }, [strokes]);

  // 上传文件转换为 base64 并加载为 Image 对象
  const handleUpload: UploadProps['onChange'] = (info) => {
    const file = info.file.originFileObj as RcFile;
    const reader = new FileReader();
    reader.onload = () => {
      const img = new Image();
      img.src = reader.result as string;
      img.onload = () => setImage(img);
    };
    reader.readAsDataURL(file);
  };

  // 计算画布缩放比例
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
      redrawCanvas(newStrokes);
    }
    setCurrentStroke([]);
  };

  // 撤销上一步操作
  const undo = () => {
    if (strokes.length === 0) return;
    const newStrokes = strokes.slice(0, -1);
    setStrokes(newStrokes);
    redrawCanvas(newStrokes);
    notifyChange();
  };

  // 重新绘制所有内容（背景图 + 所有路径）
  const redrawCanvas = (strokesList: Stroke[]) => {
    if (!image || !canvasRef.current || !ctxRef.current) return;
    const ctx = ctxRef.current;
    ctx.clearRect(0, 0, canvasRef.current.width, canvasRef.current.height);
    ctx.drawImage(image, 0, 0);
    strokesList.forEach(({ points, color, width }) => {
      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.lineWidth = width;
      points.forEach((point, index) => {
        if (index === 0) ctx.moveTo(point.x, point.y);
        else ctx.lineTo(point.x, point.y);
      });
      ctx.stroke();
    });
  };

  // 清除所有涂抹内容
  const clearStrokes = () => {
    setStrokes([]);
    redrawCanvas([]);
    notifyChange();
  };

  // 重置为未上传状态
  const resetAll = () => {
    setImage(null);
    setStrokes([]);
    const ctx = ctxRef.current;
    if (ctx && canvasRef.current)
      ctx.clearRect(0, 0, canvasRef.current.width, canvasRef.current.height);
    notifyChange(undefined);
  };

  // 通知外部 Form 更新 base64 字符串
  const notifyChange = (...args: [string?]) => {
    if (!canvasRef.current || !onChange) return;
    const base64 = args.length === 0 ? canvasRef.current.toDataURL('image/png') : args[0];
    onChange(base64);
  };

  return (
    <div className="h-[200px]">
      {/* 初始上传区域 */}
      {!image && (
        <Upload.Dragger
          showUploadList={false}
          onChange={handleUpload}
          customRequest={() => {}}
          accept="image/*"
        >
          <p className="ant-upload-drag-icon">
            <InboxOutlined />
          </p>
          <p className="ant-upload-text">{l('management.drawingTool.image.title')}</p>
          <p className="ant-upload-hint" style={{ fontSize: 12 }}>
            {l('management.drawingTool.image.desc')}
          </p>
        </Upload.Dragger>
      )}

      {/* 画布操作区域 */}
      {image && (
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
      )}
    </div>
  );
});

export default ImageEditor;
