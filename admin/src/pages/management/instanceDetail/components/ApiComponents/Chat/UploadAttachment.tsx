import { FC } from 'react';
import { Paperclip } from 'lucide-react';
import { Upload, message, Button, Tooltip } from 'antd';
import type { UploadProps } from 'antd';
import type { FileInfo } from './index';
import { l } from '@/utils/intl';

interface UploadAttachmentProps {
  fileInfo?: FileInfo;
  disabled: boolean;
  onChange: (v: FileInfo | undefined) => void;
}

/** 对话测试直传 data URL；过大易触发 nginx 默认 1m → 413 */
const MAX_IMAGE_EDGE = 2048;
const JPEG_QUALITY = 0.85;

/**
 * 根据数值和单位格式化大小
 * @param size 大小数值
 * @param unit 单位（KB, MB, GB）
 * @returns 格式化后的字符串
 */
const formatSizeWithUnit = (size: number, unit: string): string => {
  // 如果是整数，直接显示整数部分
  if (Number.isInteger(size)) {
    return `${size} ${unit}`;
  }
  // 转换为字符串，检查小数位数
  const sizeStr = size.toString();
  const decimalPart = sizeStr.split('.')[1];
  // 如果小数位数小于等于 2，直接显示
  if (decimalPart && decimalPart.length <= 2) {
    return `${size} ${unit}`;
  }
  // 否则，保留 2 位小数
  return `${size.toFixed(2)} ${unit}`;
};
/**
 * 将文件大小（字节）转换为友好格式
 * @param size 文件大小（字节）
 * @returns 格式化后的字符串，如 "2 KB", "3.21 MB", "1.5 GB"
 */
const formatFileSize = (size: number): string => {
  if (size < 1024) {
    return `${size} B`; // 小于 1KB，直接显示字节
  } else if (size < 1024 * 1024) {
    const sizeInKB = size / 1024;
    return formatSizeWithUnit(sizeInKB, 'KB');
  } else if (size < 1024 * 1024 * 1024) {
    const sizeInMB = size / (1024 * 1024);
    return formatSizeWithUnit(sizeInMB, 'MB');
  } else {
    const sizeInGB = size / (1024 * 1024 * 1024);
    return formatSizeWithUnit(sizeInGB, 'GB');
  }
};

function readFileAsDataURL(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ''));
    reader.onerror = () => reject(reader.error || new Error('read failed'));
    reader.readAsDataURL(file);
  });
}

/** 压缩图片为 JPEG data URL，失败时回退原图 data URL */
async function compressImageToDataUrl(file: File): Promise<{ url: string; byteLength: number }> {
  const originalUrl = await readFileAsDataURL(file);
  try {
    const bitmap = await createImageBitmap(file);
    const scale = Math.min(1, MAX_IMAGE_EDGE / Math.max(bitmap.width, bitmap.height));
    const width = Math.max(1, Math.round(bitmap.width * scale));
    const height = Math.max(1, Math.round(bitmap.height * scale));
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    if (!ctx) {
      bitmap.close();
      return { url: originalUrl, byteLength: file.size };
    }
    ctx.drawImage(bitmap, 0, 0, width, height);
    bitmap.close();
    const url = canvas.toDataURL('image/jpeg', JPEG_QUALITY);
    const approxBytes = Math.ceil(((url.split(',')[1] || '').length * 3) / 4);
    // 压缩后反而更大则用原图
    if (approxBytes >= file.size && scale >= 1) {
      return { url: originalUrl, byteLength: file.size };
    }
    return { url, byteLength: approxBytes };
  } catch {
    return { url: originalUrl, byteLength: file.size };
  }
}

const UploadAttachment: FC<UploadAttachmentProps> = ({ disabled, onChange }) => {
  // 直接读取文件内容,无需使用onChange+action上传
  const handleBeforeUpload: UploadProps['beforeUpload'] = (file) => {
    if (disabled) {
      return false;
    }
    const isValidType = /^(image|video|audio)\//.test(file.type); // 匹配 image/*, video/*, audio/*
    if (!isValidType) {
      message.error('只能上传图片/视频/音频文件!');
      return false;
    }
    const type = file?.type?.startsWith('audio/')
      ? 'audio'
      : file?.type?.startsWith('video/')
      ? 'video'
      : 'image';

    void (async () => {
      try {
        if (type === 'image') {
          const { url, byteLength } = await compressImageToDataUrl(file);
          onChange({
            url,
            name: file?.name,
            size: formatFileSize(byteLength || file?.size || 0),
            type,
          });
        } else {
          const url = await readFileAsDataURL(file);
          onChange({
            url,
            name: file?.name,
            size: formatFileSize(file?.size || 0),
            type,
          });
        }
        message.success('上传成功!');
      } catch {
        onChange(undefined);
        message.error(`${file?.name || ''}文件读取失败!`);
      }
    })();
    // 阻止默认上传行为
    return false;
  };
  return (
    <>
      <div className="w-[32px] h-[32px] flex items-center justify-center rounded-full cursor-pointer hover:bg-[var(--c-primary-light)] group">
        <Upload
          accept="image/*,video/*,audio/*"
          maxCount={1}
          showUploadList={false}
          beforeUpload={handleBeforeUpload}
        >
          <Tooltip title={l('model.running.chat.uploadAttachment')}>
            <Button className="border-[#fff]" shape="circle" icon={<Paperclip size={14} />} />
          </Tooltip>
        </Upload>
      </div>
    </>
  );
};
export default UploadAttachment;
