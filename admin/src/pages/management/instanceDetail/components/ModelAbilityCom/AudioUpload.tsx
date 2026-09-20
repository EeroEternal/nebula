import { Segmented, Upload as AntdUpload, Button, message } from 'antd';
import type { UploadProps, FormInstance, UploadFile } from 'antd';
import { FC, useEffect, useState, useRef, useMemo } from 'react';
import {
  Upload,
  Mic,
  Trash2,
  FileAudio,
  Play,
  Pause,
  Square,
  CircleQuestionMark,
} from 'lucide-react';
import dayjs from 'dayjs';
import { l, lGet } from '@/utils/intl';
import { IconButton, ActionWithTips } from '@/components';

const UPLOAD_METHOD = ['upload', 'record'];
interface AudioUploadProps {
  value?: UploadFile['originFileObj'] | Blob;
  form: FormInstance;
  onChange?: (v: AudioUploadProps['value']) => void;
}
const AudioUpload: FC<AudioUploadProps> = ({ value, onChange }) => {
  const [uploadType, setUploadType] = useState(UPLOAD_METHOD[0]);
  const [fileForUpload, setFileForUpload] = useState<
    { fileUrl: string; fileName: string } | undefined
  >();

  const [recordedBlob, setRecordedBlob] = useState<Blob | null>(null);
  const [isRecording, setIsRecording] = useState(false);
  const [recordingTime, setRecordingTime] = useState(0);
  const [isPaused, setIsPaused] = useState(false);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const uploadTypeOptions = useMemo(
    () => [
      {
        label: l('models.instances.detail.uploadFile'),
        value: UPLOAD_METHOD[0],
        icon: <Upload className="inline -mt-1" size={16} />,
      },
      {
        label: (
          <div className="inline-flex items-center gap-1">
            {l('models.instances.detail.recordAudio')}{' '}
            <ActionWithTips
              overlayInnerStyle={{ width: 350 }}
              title={
                <div className="whitespace-pre-line">
                  {l('models.instances.detail.audioFile.record.tips')}
                </div>
              }
            >
              <CircleQuestionMark size={16} className="text-muted" />
            </ActionWithTips>
          </div>
        ),
        value: UPLOAD_METHOD[1],
        icon: <Mic className="inline -mt-1" size={16} />,
      },
    ],
    [],
  );

  const handleTypeChange = (value: string) => {
    setUploadType(value);
    onChange?.(undefined);
  };
  const handleDelete = (e: React.MouseEvent<HTMLDivElement>) => {
    e.stopPropagation();
    setFileForUpload(undefined);
    onChange?.(undefined);
  };
  const handleBeforeUpload: UploadProps['beforeUpload'] = (file) => {
    const fileUrl = URL.createObjectURL(file);
    setFileForUpload({
      fileName: file.name,
      fileUrl,
    });
    onChange?.(file);
    return false;
  };
  const startRecording = async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mediaRecorder = new MediaRecorder(stream);
      mediaRecorderRef.current = mediaRecorder;
      chunksRef.current = [];

      mediaRecorder.ondataavailable = (e) => {
        if (e.data.size > 0) chunksRef.current.push(e.data);
      };

      mediaRecorder.onstop = () => {
        const blob = new Blob(chunksRef.current, { type: 'audio/webm' });
        setRecordedBlob(blob);
        onChange?.(blob);
        stream.getTracks().forEach((t) => t.stop());
      };

      mediaRecorder.start();
      setIsRecording(true);
      setIsPaused(false);
      setRecordingTime(0);
      setRecordedBlob(null);
      timerRef.current = setInterval(() => setRecordingTime((t) => t + 1), 1000);
    } catch {
      message.error(lGet('models.instances.detail.audioFile.record.error'));
    }
  };
  const formatTime = () => {
    if (recordingTime > 3599) {
      return dayjs.duration(recordingTime, 'seconds').format('HH:mm:ss');
    }
    return dayjs.duration(recordingTime, 'seconds').format('mm:ss');
  };
  const pauseRecording = () => {
    if (mediaRecorderRef.current?.state === 'recording') {
      mediaRecorderRef.current.pause();
      setIsPaused(true);
      if (timerRef.current) clearInterval(timerRef.current);
    }
  };
  const resumeRecording = () => {
    if (mediaRecorderRef.current?.state === 'paused') {
      mediaRecorderRef.current.resume();
      setIsPaused(false);
      timerRef.current = setInterval(() => setRecordingTime((t) => t + 1), 1000);
    }
  };
  const stopRecording = () => {
    if (mediaRecorderRef.current && mediaRecorderRef.current.state !== 'inactive') {
      mediaRecorderRef.current.stop();
    }
    setIsRecording(false);
    setIsPaused(false);
    if (timerRef.current) clearInterval(timerRef.current);
  };
  const reRecord = () => {
    onChange?.(undefined);
    setRecordedBlob(null);
    setRecordingTime(0);
    startRecording();
  };
  const deleteRecord = () => {
    onChange?.(undefined);
    setRecordedBlob(null);
    setRecordingTime(0);
  };
  useEffect(() => {
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
      if (mediaRecorderRef.current && mediaRecorderRef.current.state !== 'inactive') {
        mediaRecorderRef.current.stop();
      }
    };
  }, []);
  useEffect(() => {
    if (!value) {
      setFileForUpload(undefined);
      setRecordedBlob(null);
      setRecordingTime(0);
    }
  }, [value]);
  return (
    <div>
      <Segmented
        block
        size="large"
        className="mb-3"
        value={uploadType}
        options={uploadTypeOptions}
        onChange={handleTypeChange}
      />
      {uploadType === UPLOAD_METHOD[0] ? (
        <AntdUpload.Dragger
          name="file"
          multiple={false}
          accept="audio/*"
          beforeUpload={handleBeforeUpload}
          showUploadList={false}
          iconRender={() => <Upload size={35} className="text-primary" />}
        >
          {fileForUpload ? (
            <div className="m-auto flex flex-col items-center gap-2">
              <div className="h-12 w-12 rounded-full bg-primary/10 flex items-center justify-center">
                <Upload className="h-5 w-5 text-primary" />
              </div>
              <audio controls src={fileForUpload.fileUrl} className=""></audio>
              <div className="text-muted">{fileForUpload?.fileName}</div>
              <IconButton
                onClick={handleDelete}
                className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
              >
                <Trash2 size={16} />
              </IconButton>
            </div>
          ) : (
            <div className="m-auto flex flex-col items-center">
              <div className="h-12 w-12 rounded-full bg-primary/10 flex items-center justify-center mb-3">
                <Upload className="h-5 w-5 text-primary" />
              </div>
              <div className="text-base font-medium mb-1">
                {l('models.instances.detail.uploadAudioTitle')}
              </div>
              <div className="text-muted">{l('models.instances.detail.uploadAudioDesc')}</div>
            </div>
          )}
        </AntdUpload.Dragger>
      ) : (
        <div className="flex flex-col items-center justify-center border border-dashed rounded-lg p-4 gap-4 transition-all bg-background/50 relative">
          <div className="relative">
            <div
              className={`h-12 w-12 rounded-full  flex items-center justify-center ${
                isRecording ? 'bg-danger/10' : 'bg-primary/10'
              }`}
            >
              {recordedBlob && !isRecording ? (
                <FileAudio size={20} className="text-primary" />
              ) : (
                <Mic size={20} className={`${isRecording ? 'text-danger' : 'text-primary'}`} />
              )}
            </div>
            {isRecording && !isPaused && (
              <>
                <div className="absolute inset-0 rounded-full border-2 border-danger/30 animate-ping" />
                <div className="absolute -inset-1 rounded-full border border-danger/15 animate-pulse" />
              </>
            )}
          </div>
          {(isRecording || recordedBlob) && (
            <p className="font-mono text-lg font-medium text-foreground">{formatTime()}</p>
          )}
          <p className="text-sm text-muted">
            {isRecording
              ? isPaused
                ? l('models.instances.detail.audioFile.record.paused')
                : l('models.instances.detail.audioFile.record.ing')
              : recordedBlob
              ? l('models.instances.detail.audioFile.record.done')
              : l('models.instances.detail.audioFile.record.default')}
          </p>

          <div className="flex items-center gap-2">
            {!isRecording && !recordedBlob && (
              <Button type="primary" onClick={startRecording} icon={<Mic size={14} />}>
                {l('models.instances.detail.audioFile.record.start')}
              </Button>
            )}
            {isRecording && (
              <>
                <Button
                  icon={isPaused ? <Play size={14} /> : <Pause size={14} />}
                  onClick={isPaused ? resumeRecording : pauseRecording}
                />
                <Button type="primary" danger onClick={stopRecording} icon={<Square size={14} />}>
                  {l('models.instances.detail.audioFile.record.stop')}
                </Button>
              </>
            )}
            {recordedBlob && !isRecording && (
              <>
                <Button icon={<Mic size={14} />} onClick={reRecord}>
                  {l('models.instances.detail.audioFile.record.reRecord')}
                </Button>
              </>
            )}
            {recordedBlob && !isRecording && (
              <IconButton
                onClick={deleteRecord}
                className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
              >
                <Trash2 size={16} />
              </IconButton>
            )}
          </div>
        </div>
      )}
    </div>
  );
};
export default AudioUpload;
