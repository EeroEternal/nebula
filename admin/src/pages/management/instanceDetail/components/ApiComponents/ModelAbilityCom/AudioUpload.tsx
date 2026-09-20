import { Upload, Form, Button, message } from 'antd';
import type { UploadProps, FormInstance, UploadFile } from 'antd';
import { FC, useEffect, useState, useRef } from 'react';
import { InboxOutlined, DeleteOutlined } from '@ant-design/icons';
import { l, lGet } from '@/utils/intl';
import { UPLOAD_METHOD } from '@/constants/modelData';

interface AudioUploadProps {
  value?: UploadFile['originFileObj'] | Blob;
  form: FormInstance;
  onChange?: (v: AudioUploadProps['value']) => void;
}
const AudioUpload: FC<AudioUploadProps> = ({ form, value, onChange }) => {
  const uploadType = Form.useWatch('uploadType', form);
  const [fileUrl, setFileUrl] = useState<string | undefined>(undefined);
  const [isRecording, setIsRecording] = useState(false);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const handleBeforeUpload: UploadProps['beforeUpload'] = (file) => {
    const fileUrl = URL.createObjectURL(file);
    setFileUrl(fileUrl);
    onChange?.(file);
    return false;
  };
  const handleRecording = async () => {
    setIsRecording(!isRecording);
    if (isRecording) {
      if (mediaRecorderRef.current) {
        mediaRecorderRef.current.stop();
        mediaRecorderRef.current = null;
      }
      setIsRecording(false);
    } else {
      // 开始录音
      onChange?.(undefined);
      setFileUrl(undefined);
      if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        mediaRecorderRef.current = new MediaRecorder(stream);
        const audioChunks: Blob[] = [];
        mediaRecorderRef.current.ondataavailable = (event) => {
          audioChunks.push(event.data);
        };

        mediaRecorderRef.current.onstop = () => {
          const audioBlob = new Blob(audioChunks, { type: 'audio/wav' });
          onChange?.(audioBlob);
          const url = URL.createObjectURL(audioBlob);
          setFileUrl(url);
        };
        mediaRecorderRef.current.start();
        setIsRecording(true);
      } else {
        setIsRecording(false);
        message.error(lGet('model.running.recordingErrorTips'));
      }
    }
  };
  const handleDelete = () => {
    onChange?.(undefined);
    setFileUrl(undefined);
  };
  useEffect(() => {
    if (!value) setFileUrl(undefined);
  }, [value]);
  return (
    <div className="px-[4px]">
      {uploadType === UPLOAD_METHOD[1].value ? (
        <Button
          danger={isRecording}
          type="primary"
          block
          onClick={handleRecording}
          className="my-[8px]"
        >
          {l(isRecording ? 'model.running.stop' : 'model.running.start')}
        </Button>
      ) : (
        <Upload.Dragger
          name="file"
          multiple={false}
          accept="audio/*"
          beforeUpload={handleBeforeUpload}
          showUploadList={false}
        >
          <p className="ant-upload-drag-icon">
            <InboxOutlined />
          </p>
          <p className="ant-upload-text">{l('model.running.uploadDragTitle')}</p>
        </Upload.Dragger>
      )}

      {fileUrl && (
        <div className="flex items-center gap-x-[10px]">
          <audio controls src={fileUrl} className="w-full my-[8px]" />
          <Button type="text" icon={<DeleteOutlined />} onClick={handleDelete}></Button>
        </div>
      )}
    </div>
  );
};
export default AudioUpload;
