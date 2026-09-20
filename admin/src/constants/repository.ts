import { ModelType } from './modelData';

export const TABS = ['LLM', 'image', 'embedding', 'rerank', 'audio', 'video', 'custom'];

/** 仓库顶栏：类型 Tab + 注册模型右侧的「模型下载」进度页 */
export const REPOSITORY_NAV_TABS = [...TABS, 'downloads'] as const;
export const FILTER_STATUS = ['cached'];

export const FILTER_MODEL_ABILITY_MAP = {
  [ModelType.LLM]: ['generate', 'chat', 'vision', 'reasoning', 'tools', 'hybrid', 'audio', 'omni'],
  [ModelType.image]: ['text2image', 'image2image', 'inpainting', 'ocr'],
  [ModelType.audio]: [
    'audio2text',
    'text2audio',
    'text2audio_zero_shot',
    'text2audio_voice_cloning',
    'text2audio_emotion_control',
  ],
  [ModelType.video]: ['text2video', 'image2video', 'firstlastframe2video', 'video2text'],
} as Record<ModelType, string[]>;

export const FILTER_MARKS = {
  0: 0,
  8: '8K',
  32: '32K',
  100: '>100K',
};
export const FILTER_LANG_MAP = {
  [ModelType.LLM]: ['zh', 'en', 'ko', 'ja', 'es', 'de', 'it'],
  [ModelType.embedding]: ['zh', 'en'],
  [ModelType.rerank]: ['zh', 'en'],
} as Record<ModelType, string[]>;

export const FILTER_MODEL_TYPE = ['LLM', 'image', 'embedding', 'rerank', 'audio', 'flexible'];

export const UPDATE_MODEL_TYPE = [
  { label: 'LLM', value: 'LLM' },
  { label: 'Image', value: 'image' },
  { label: 'Embedding', value: 'embedding' },
  { label: 'Rerank', value: 'rerank' },
  { label: 'Audio', value: 'audio' },
  { label: 'Video', value: 'video' },
];
