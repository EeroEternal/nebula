import { lGet } from '@/utils/intl';
import type { AutoCompleteProps } from 'antd';

export const detailedTabWithoutBuiltin = [
  {
    key: 'LLM',
  },
  {
    key: 'image',
  },
  {
    key: 'embedding',
  },
  {
    key: 'rerank',
  },
  {
    key: 'audio',
  },
  {
    tab: '视频模型',
    key: 'video',
  },
  {
    tab: '其他模型',
    key: 'flexible',
  },
];
export const customTab = [
  {
    key: 'LLM',
  },
  {
    key: 'image',
  },
  {
    key: 'embedding',
  },
  {
    key: 'rerank',
  },
  // {
  //   key: 'audio',
  // },
  {
    key: 'video',
  },
];
export const REGISTER_TABS = ['LLM', 'image', 'embedding', 'rerank', 'audio', 'flexible'];
// 注册模型tab
export const registerTab = [
  {
    tab: '大语言模型',
    key: 'LLM',
  },
  {
    tab: '图像模型',
    key: 'image',
  },
  {
    tab: 'Embedding模型',
    key: 'embedding',
  },
  {
    tab: 'ReRank模型',
    key: 'rerank',
  },
  {
    tab: '音频模型',
    key: 'audio',
  },
  // {
  //   tab: '视频模型',
  //   key: 'video',
  // },
  {
    tab: '其他模型',
    key: 'flexible',
  },
];

export const modalTagLang = {
  en: '英文',
  zh: '中文',
  es: '西班牙语',
  de: '德语',
  it: '意大利语',
};
export const tagAbility = {
  LLM: {
    generate: '生成',
    chat: '对话',
    tools: '工具',
    vision: '视觉',
  },
  embedding: {
    embed: '嵌入',
  },
  rerank: {
    rerank: '重排',
  },
  image: {
    text_to_image: '文生图',
  },
  audio: {},
};

export const LLMFilterOptions = ['model_abililty', 'content_length', 'model_lang'];

export const EmbeddingOptions = ['model_ability', 'model_lang'];

export const rerankOptions = ['model_ability', 'model_lang'];

export const imgOptions = ['model_ability'];
export const modelTypeOptions = [
  {
    value: 'LLM',
    label: '大语言模型',
  },
  {
    value: 'embedding',
    label: 'Embedding模型',
  },
  {
    value: 'rerank',
    label: 'ReRank模型',
  },
  {
    value: 'image',
    label: '图像模型',
  },
  {
    value: 'audio',
    label: '音频模型',
  },
  // {
  //   value: 'video',
  //   label: '视频模型',
  // },
  // {
  //   value: 'flexible',
  //   label: '其他模型',
  // },
];

export enum ModelType {
  LLM = 'LLM',
  image = 'image',
  embedding = 'embedding',
  rerank = 'rerank',
  audio = 'audio',
  video = 'video',
  flexible = 'flexible',
}
export type ModelRegisterFormKey = Record<
  ModelType,
  {
    model_name?: string;
    version?: number;
    model_description?: string;
    context_length?: number;
    model_lang?: string[];
    model_ability?: string | string[];
    model_family?: string | string[];
    model_specs?: {
      model_uri: string;
      model_size_in_billions?: string;
      model_format: string;
      quantization: string;
    }[];
    prompt_style?: unknown;
    model_uri?: string;
    controlnet?: {
      model_name: string;
      model_uri: string;
      model_format: string;
    }[];
    dimensions?: number;
    max_tokens?: number;
    language?: string[];
    multilingual?: boolean;
    model_hub?: string;
    launcher?: string;
    launcher_args?: string;
    virtualenv: {
      packages: { value: string }[];
    };
    worker_ip?: string[];
  }
>;
export const MODEL_DEFAULT_VALUE: ModelRegisterFormKey = {
  [ModelType.LLM]: {
    version: 2,
    model_name: 'custom-llm',
    model_description: '模型描述（请按实际填写）',
    context_length: 2048,
    model_lang: ['zh'],
    model_ability: ['generate'],
    model_family: '',
    model_specs: [
      {
        model_uri: '/path/to/model',
        model_size_in_billions: '7',
        model_format: 'pytorch',
        quantization: 'none',
      },
    ],
    prompt_style: undefined,
    virtualenv: {
      packages: [],
    },
    worker_ip: [],
  },
  [ModelType.image]: {
    model_name: 'custom-image',
    model_uri: '/path/to/image-model',
    model_family: 'stable_diffusion',
    controlnet: [],
    virtualenv: {
      packages: [],
    },
    model_ability: [],
    worker_ip: [],
  },
  [ModelType.embedding]: {
    model_name: 'custom-embedding',
    dimensions: 768,
    max_tokens: 512,
    language: ['en'],
    model_specs: [
      {
        model_uri: '/path/to/llama-1',
        model_format: 'pytorch',
        quantization: 'none',
      },
    ],
    virtualenv: {
      packages: [],
    },
    worker_ip: [],
  },
  [ModelType.rerank]: {
    model_name: 'custom-rerank',
    language: ['en'],
    max_tokens: 512,
    model_specs: [
      {
        model_uri: '/path/to/rerank-1',
        model_format: 'pytorch',
        quantization: 'none',
      },
    ],
    virtualenv: {
      packages: [],
    },
    worker_ip: [],
  },
  [ModelType.audio]: {
    model_name: 'custom-audio',
    model_uri: '/path/to/audio-model',
    multilingual: false,
    model_ability: 'text2audio',
    model_family: 'whisper',
    virtualenv: {
      packages: [],
    },
    worker_ip: [],
  },
  [ModelType.video]: {
    virtualenv: {
      packages: [],
    },
    worker_ip: [],
  },
  [ModelType.flexible]: {
    model_name: 'flexible-model',
    model_uri: '/path/to/model',
    model_hub: 'huggingface',
    model_description: '模型描述（请按实际填写）',
    launcher: 'powerllm.model.flexible.launchers.transformers',
    launcher_args: '{}',
    virtualenv: {
      packages: [],
    },
    worker_ip: [],
  },
};

export const LANGUAGES_OPTIONS = [
  { label: 'English', value: 'en' },
  { label: 'Chinese', value: 'zh' },
];

export const ALL_LANGUAGES_OPTIONS = [
  { value: 'ab', label: 'Abkhazian' },
  { value: 'aa', label: 'Afar' },
  { value: 'af', label: 'Afrikaans' },
  { value: 'ak', label: 'Akan' },
  { value: 'sq', label: 'Albanian' },
  { value: 'am', label: 'Amharic' },
  { value: 'ar', label: 'Arabic' },
  { value: 'an', label: 'Aragonese' },
  { value: 'hy', label: 'Armenian' },
  { value: 'as', label: 'Assamese' },
  { value: 'av', label: 'Avaric' },
  { value: 'ae', label: 'Avestan' },
  { value: 'ay', label: 'Aymara' },
  { value: 'az', label: 'Azerbaijani' },
  { value: 'bm', label: 'Bambara' },
  { value: 'ba', label: 'Bashkir' },
  { value: 'eu', label: 'Basque' },
  { value: 'be', label: 'Belarusian' },
  { value: 'bn', label: 'Bengali' },
  { value: 'bh', label: 'Bihari' },
  { value: 'bi', label: 'Bislama' },
  { value: 'bs', label: 'Bosnian' },
  { value: 'br', label: 'Breton' },
  { value: 'bg', label: 'Bulgarian' },
  { value: 'my', label: 'Burmese' },
  { value: 'ca', label: 'Catalan, Valencian' },
  { value: 'ch', label: 'Chamorro' },
  { value: 'ce', label: 'Chechen' },
  { value: 'ny', label: 'Chichewa, Chewa, Nyanja' },
  // { value: 'zh', label: 'Chinese' },
  { value: 'cv', label: 'Chuvash' },
  { value: 'kw', label: 'Cornish' },
  { value: 'co', label: 'Corsican' },
  { value: 'cr', label: 'Cree' },
  { value: 'hr', label: 'Croatian' },
  { value: 'cs', label: 'Czech' },
  { value: 'da', label: 'Danish' },
  { value: 'dv', label: 'Divehi, Dhivehi, Maldivian' },
  { value: 'nl', label: 'Dutch, Flemish' },
  { value: 'dz', label: 'Dzongkha' },
  // { value: 'en', label: 'English' },
  { value: 'eo', label: 'Esperanto' },
  { value: 'et', label: 'Estonian' },
  { value: 'ee', label: 'Ewe' },
  { value: 'fo', label: 'Faroese' },
  { value: 'fj', label: 'Fijian' },
  { value: 'fi', label: 'Finnish' },
  { value: 'fr', label: 'French' },
  { value: 'ff', label: 'Fulah' },
  { value: 'gl', label: 'Galician' },
  { value: 'ka', label: 'Georgian' },
  { value: 'de', label: 'German' },
  { value: 'el', label: 'Greek' },
  { value: 'gn', label: 'Guarani' },
  { value: 'gu', label: 'Gujarati' },
  { value: 'ht', label: 'Haitian, Haitian Creole' },
  { value: 'ha', label: 'Hausa' },
  { value: 'he', label: 'Hebrew' },
  { value: 'hz', label: 'Herero' },
  { value: 'hi', label: 'Hindi' },
  { value: 'ho', label: 'Hiri Motu' },
  { value: 'hu', label: 'Hungarian' },
  { value: 'ia', label: 'Interlingua' },
  { value: 'id', label: 'Indonesian' },
  { value: 'ie', label: 'Interlingue, Occidental' },
  { value: 'ga', label: 'Irish' },
  { value: 'ig', label: 'Igbo' },
  { value: 'ik', label: 'Inupiaq' },
  { value: 'io', label: 'Ido' },
  { value: 'is', label: 'Icelandic' },
  { value: 'it', label: 'Italian' },
  { value: 'iu', label: 'Inuktitut' },
  { value: 'ja', label: 'Japanese' },
  { value: 'jv', label: 'Javanese' },
  { value: 'kl', label: 'Kalaallisut, Greenlandic' },
  { value: 'kn', label: 'Kannada' },
  { value: 'kr', label: 'Kanuri' },
  { value: 'ks', label: 'Kashmiri' },
  { value: 'kk', label: 'Kazakh' },
  { value: 'km', label: 'Central Khmer' },
  { value: 'ki', label: 'Kikuyu, Gikuyu' },
  { value: 'rw', label: 'Kinyarwanda' },
  { value: 'ky', label: 'Kirghiz, Kyrgyz' },
  { value: 'kv', label: 'Komi' },
  { value: 'kg', label: 'Kongo' },
  { value: 'ko', label: 'Korean' },
  { value: 'ku', label: 'Kurdish' },
  { value: 'kj', label: 'Kuanyama, Kwanyama' },
  { value: 'la', label: 'Latin' },
  { value: 'lb', label: 'Luxembourgish, Letzeburgesch' },
  { value: 'lg', label: 'Ganda' },
  { value: 'li', label: 'Limburgan, Limburger, Limburgish' },
  { value: 'ln', label: 'Lingala' },
  { value: 'lo', label: 'Lao' },
  { value: 'lt', label: 'Lithuanian' },
  { value: 'lu', label: 'Luba-Katanga' },
  { value: 'lv', label: 'Latvian' },
  { value: 'gv', label: 'Manx' },
  { value: 'mk', label: 'Macedonian' },
  { value: 'mg', label: 'Malagasy' },
  { value: 'ms', label: 'Malay' },
  { value: 'ml', label: 'Malayalam' },
  { value: 'mt', label: 'Maltese' },
  { value: 'mi', label: 'Maori' },
  { value: 'mr', label: 'Marathi' },
  { value: 'mh', label: 'Marshallese' },
  { value: 'mn', label: 'Mongolian' },
  { value: 'na', label: 'Nauru' },
  { value: 'nv', label: 'Navajo, Navaho' },
  { value: 'nd', label: 'North Ndebele' },
  { value: 'ne', label: 'Nepali' },
  { value: 'ng', label: 'Ndonga' },
  { value: 'nb', label: 'Norwegian Bokmål' },
  { value: 'nn', label: 'Norwegian Nynorsk' },
  { value: 'no', label: 'Norwegian' },
  { value: 'ii', label: 'Sichuan Yi, Nuosu' },
  { value: 'nr', label: 'South Ndebele' },
  { value: 'oc', label: 'Occitan' },
  { value: 'oj', label: 'Ojibwa' },
  {
    value: 'cu',
    label: 'Church Slavic, Old Slavonic, Church Slavonic, Old Bulgarian, Old Church Slavonic',
  },
  { value: 'om', label: 'Oromo' },
  { value: 'or', label: 'Oriya' },
  { value: 'os', label: 'Ossetian, Ossetic' },
  { value: 'pa', label: 'Punjabi, Panjabi' },
  { value: 'pi', label: 'Pali' },
  { value: 'fa', label: 'Persian' },
  { value: 'pl', label: 'Polish' },
  { value: 'ps', label: 'Pashto, Pushto' },
  { value: 'pt', label: 'Portuguese' },
  { value: 'qu', label: 'Quechua' },
  { value: 'rm', label: 'Romansh' },
  { value: 'rn', label: 'Rundi' },
  { value: 'ro', label: 'Romanian, Moldavian, Moldovan' },
  { value: 'ru', label: 'Russian' },
  { value: 'sa', label: 'Sanskrit' },
  { value: 'sc', label: 'Sardinian' },
  { value: 'sd', label: 'Sindhi' },
  { value: 'se', label: 'Northern Sami' },
  { value: 'sm', label: 'Samoan' },
  { value: 'sg', label: 'Sango' },
  { value: 'sr', label: 'Serbian' },
  { value: 'gd', label: 'Gaelic, Scottish Gaelic' },
  { value: 'sn', label: 'Shona' },
  { value: 'si', label: 'Sinhala, Sinhalese' },
  { value: 'sk', label: 'Slovak' },
  { value: 'sl', label: 'Slovenian' },
  { value: 'so', label: 'Somali' },
  { value: 'st', label: 'Southern Sotho' },
  { value: 'es', label: 'Spanish, Castilian' },
  { value: 'su', label: 'Sundanese' },
  { value: 'sw', label: 'Swahili' },
  { value: 'ss', label: 'Swati' },
  { value: 'sv', label: 'Swedish' },
  { value: 'ta', label: 'Tamil' },
  { value: 'te', label: 'Telugu' },
  { value: 'tg', label: 'Tajik' },
  { value: 'th', label: 'Thai' },
  { value: 'ti', label: 'Tigrinya' },
  { value: 'bo', label: 'Tibetan' },
  { value: 'tk', label: 'Turkmen' },
  { value: 'tl', label: 'Tagalog' },
  { value: 'tn', label: 'Tswana' },
  { value: 'to', label: 'Tonga' },
  { value: 'tr', label: 'Turkish' },
  { value: 'ts', label: 'Tsonga' },
  { value: 'tt', label: 'Tatar' },
  { value: 'tw', label: 'Twi' },
  { value: 'ty', label: 'Tahitian' },
  { value: 'ug', label: 'Uighur, Uyghur' },
  { value: 'uk', label: 'Ukrainian' },
  { value: 'ur', label: 'Urdu' },
  { value: 'uz', label: 'Uzbek' },
  { value: 've', label: 'Venda' },
  { value: 'vi', label: 'Vietnamese' },
  { value: 'vo', label: 'Volapük' },
  { value: 'wa', label: 'Walloon' },
  { value: 'cy', label: 'Welsh' },
  { value: 'wo', label: 'Wolof' },
  { value: 'fy', label: 'West Frisian' },
  { value: 'xh', label: 'Xhosa' },
  { value: 'yi', label: 'Yiddish' },
  { value: 'yo', label: 'Yoruba' },
  { value: 'za', label: 'Zhuang, Chuang' },
  { value: 'zu', label: 'Zulu' },
];
/**模型属性 */
export const MODEL_ABILITY_OPTIONS = [
  { label: 'Generate', value: 'generate' },
  { label: 'Chat', value: 'chat' },
  { label: 'Vision', value: 'vision' },
  { label: 'Tools', value: 'tools' },
];
/** 模型系列-（image） */
export const MODEL_FAMILY_IMAGE_OPTIONS = [
  { value: 'stable_diffusion', label: 'stable_diffusion' },
  { value: 'ocr', label: 'ocr' },
];
/** 模型能力-（audio） */
export const MODEL_ABILITY_AUDIO_OPTIONS = ['text2audio', 'audio2text'];
/** 模型能力-（image） */
export const MODEL_ABILITY_IMAGE_OPTIONS = ['text2image', 'image2image', 'inpainting', 'ocr'];
/** 模型系列-（audio） */
export const MODEL_FAMILY_AUDIO_OPTIONS = [
  'whisper',
  'ChatTTS',
  'CosyVoice',
  'F5-TTS',
  'F5-TTS-MLX',
  'FishAudio',
  'Kokoro',
  'MegaTTS',
  'MeloTTS',
  'funasr',
];
/** 模型格式(LLM) */
export const MODEL_FORMAT_OPTIONS = {
  [ModelType.LLM]: [
    { value: 'pytorch', label: 'PyTorch' },
    { value: 'ggufv2', label: 'GGUF' },
    { value: 'gptq', label: 'GPTQ' },
    { value: 'awq', label: 'AWQ' },
    { value: 'fp8', label: 'FP8' },
    { value: 'mlx', label: 'MLX' },
  ],
  [ModelType.embedding]: [
    { value: 'pytorch', label: 'PyTorch' },
    { value: 'ggufv2', label: 'GGUF' },
  ],
  [ModelType.rerank]: [
    { value: 'pytorch', label: 'PyTorch' },
    // { value: 'ggufv2', label: 'GGUF' },
  ],
};
/** 模型格式(IMAGE) */
export const MODEL_FORMAT_IMAGE_OPTIONS = [{ value: 'controlnet', label: 'controlnet' }];

/** chat_templat测试消息实例 */
export const MODEL_TEMPLATE_TEST_MSG = [
  {
    role: 'assistant',
    content: 'This is the message content replied by the assistant previously',
  },
  {
    role: 'user',
    content: 'This is the message content sent by the user currently',
  },
];

/** 延迟取文案，避免模块初始化阶段与 intl 循环依赖导致 lGet 未就绪 */
const suggestCommonParamsLabel = () =>
  typeof lGet === 'function'
    ? (lGet('global.tips.suggestsCommonParameters') as string)
    : '常用参数建议';

const withSuggestGroup = (options: Array<{ label: string; value: string }>) => [
  {
    get label() {
      return suggestCommonParamsLabel();
    },
    title: '',
    options,
  },
];

/** 部署模型-其他参数（AutoComplete）的预选项 */
export const MODEL_EXTENDS_CONFIG_TEMPLATE = {
  transformers: withSuggestGroup([
    { label: 'torch_dtype', value: 'torch_dtype' },
    { label: 'device', value: 'device' },
  ]),
  'llama.cpp': withSuggestGroup([
    { label: 'n_ctx', value: 'n_ctx' },
    { label: 'use_mmap', value: 'use_mmap' },
    { label: 'use_mlock', value: 'use_mlock' },
  ]),
  vllm: withSuggestGroup([
    { label: 'max_model_len', value: 'max_model_len' },
    { label: 'gpu_memory_utilization', value: 'gpu_memory_utilization' },
    { label: 'max_num_seqs', value: 'max_num_seqs' },
    { label: 'tool_call_parser', value: 'tool_call_parser' },
    { label: 'dp', value: 'dp' },
    { label: 'dtype', value: 'dtype' },
    { label: 'block_size', value: 'block_size' },
    { label: 'guided_decoding_backend', value: 'guided_decoding_backend' },
    { label: 'scheduling_policy', value: 'scheduling_policy' },
    { label: 'tensor_parallel_size', value: 'tensor_parallel_size' },
    { label: 'pipeline_parallel_size', value: 'pipeline_parallel_size' },
    { label: 'enable_prefix_caching', value: 'enable_prefix_caching' },
    { label: 'enable_chunked_prefill', value: 'enable_chunked_prefill' },
    { label: 'enforce_eager', value: 'enforce_eager' },
    { label: 'cpu_offload_gb', value: 'cpu_offload_gb' },
    { label: 'disable_custom_all_reduce', value: 'disable_custom_all_reduce' },
    { label: 'limit_mm_per_prompt', value: 'limit_mm_per_prompt' },
    { label: 'model_quantization', value: 'model_quantization' },
  ]),
  sglang: withSuggestGroup([
    { label: 'mem_fraction_static', value: 'mem_fraction_static' },
    { label: 'attention_reduce_in_fp32', value: 'attention_reduce_in_fp32' },
    { label: 'tp_size', value: 'tp_size' },
    { label: 'pp_size', value: 'pp_size' },
    { label: 'pipeline_parallel_size', value: 'pipeline_parallel_size' },
    { label: 'nnodes', value: 'nnodes' },
    { label: 'dp_size', value: 'dp_size' },
    { label: 'chunked_prefill_size', value: 'chunked_prefill_size' },
    { label: 'cpu_offload_gb', value: 'cpu_offload_gb' },
    { label: 'enable_dp_attention', value: 'enable_dp_attention' },
    { label: 'enable_ep_moe', value: 'enable_ep_moe' },
  ]),
  mlx: withSuggestGroup([
    { label: 'cache_limit_gb', value: 'cache_limit_gb' },
    { label: 'max_kv_size', value: 'max_kv_size' },
  ]),
};
export const MODEL_QUANTIZATION_TEMPLATE = withSuggestGroup([
  { label: 'load_in_8bit', value: 'load_in_8bit' },
  { label: 'load_in_4bit', value: 'load_in_4bit' },
  { label: 'llm_int8_threshold', value: 'llm_int8_threshold' },
  { label: 'llm_int8_skip_modules', value: 'llm_int8_skip_modules' },
  { label: 'llm_int8_enable_fp32_cpu_offload', value: 'llm_int8_enable_fp32_cpu_offload' },
  { label: 'llm_int8_has_fp16_weight', value: 'llm_int8_has_fp16_weight' },
  { label: 'bnb_4bit_compute_dtype', value: 'bnb_4bit_compute_dtype' },
  { label: 'bnb_4bit_quant_type', value: 'bnb_4bit_quant_type' },
  { label: 'bnb_4bit_use_double_quant', value: 'bnb_4bit_use_double_quant' },
  { label: 'bnb_4bit_quant_storage', value: 'bnb_4bit_quant_storage' },
]);
export const MODEL_IMAGE_KWARGS_OPTION = withSuggestGroup([
  { label: 'guidance_scale', value: 'guidance_scale' },
  { label: 'num_inference_steps', value: 'num_inference_steps' },
  { label: 'sampler_name', value: 'sampler_name' },
]);
export const MODEL_IMAGE_TO_VIDEO_KWARGS_OPTION = withSuggestGroup([
  { label: 'width', value: 'width' },
  { label: 'height', value: 'height' },
  { label: 'fps', value: 'fps' },
  { label: 'num_frames', value: 'num_frames' },
  { label: 'guidance_scale', value: 'guidance_scale' },
]);

/** 生成的数量 */
export const GENERATIONS_NUMBER = [1, 2, 3, 4];
/** 返回格式 */
export const RESPONSE_FORMAT = ['b64_json', 'url'];
/** 上传方式 */
export const UPLOAD_METHOD = [
  { label: '上传', value: 'upload' },
  { label: '录音', value: 'recording' },
];
/** 模型能力 */
export enum ModelAbility {
  /** 生成 */
  generate = 'generate',
  /** 对话 */
  chat = 'chat',
  /** 嵌入 */
  embed = 'embed',
  /** 重排 */
  rerank = 'rerank',
  /** 文生图 */
  text2image = 'text2image',
  /** 图生图 */
  image2image = 'image2image',
  /** 图片修复 */
  inpainting = 'inpainting',
  /** 音频转文字 */
  audio2text = 'audio2text',
  /** 文字转音频 */
  text2audio = 'text2audio',
  /** 音频克隆 */
  audio2audio = 'audio2audio',
  /** 工具 */
  tools = 'tools',
  /** 图像识别 */
  vision = 'vision',
  /** 文字转视频 */
  text2video = 'text2video',
  /** 文字转视频 */
  image2video = 'image2video',
  /** 音频 */
  audio = 'audio',
  /** 思考 */
  reasoning = 'reasoning',
  /** 全能 */
  omni = 'omni',
  /** 图片中提取出文字信息 */
  ocr = 'ocr',
  /** 首尾生视频 */
  flf2v = 'firstlastframe2video',
  /** 零样本文本转音频*/
  text2audioVoiceCloning = 'text2audio_voice_cloning',
  /** 语音克隆 */
  text2audioZeroShot = 'text2audio_zero_shot',
  /** 情感控制 */
  text2audioEmotionControl = 'text2audio_emotion_control',
  /** 文档解析 */
  docanalyze = 'docanalyze',
  /** 混合能力 */
  hybrid = 'hybrid',
}

export const ABILITY_ARRAY: ModelAbility[] = Object.values(ModelAbility);

/** 模型实例-扩展参数（预选项） */
export const MODEL_KWARGS_OPTIONS = {
  // [ModelAbility.text2image]: MODEL_IMAGE_KWARGS_OPTION,
  // [ModelAbility.image2image]: MODEL_IMAGE_KWARGS_OPTION,
  // [ModelAbility.inpainting]: MODEL_IMAGE_KWARGS_OPTION,
  // [ModelAbility.image2video]: MODEL_IMAGE_TO_VIDEO_KWARGS_OPTION,
} as Record<ModelAbility, AutoCompleteProps['options']>;
/** 模型虚拟空间 */
export const MODEL_VIRTUAL_ENABLE_OPTIONS = [
  { label: 'Unset', value: 'Unset' },
  { label: 'False', value: false },
  { label: 'True', value: true },
];
export const VIRTUAL_ENV_UNSET = MODEL_VIRTUAL_ENABLE_OPTIONS[0].value;
/** 模型来源（与 DownloadDrawer 默认对齐；优先用模型详情 download_hubs） */
export const MODEL_DOWNLOAD_HUB = ['modelscope', 'huggingface', 'csghub'];

export const MODEL_DOWNLOAD_HUB_LABELS: Record<string, string> = {
  modelscope: 'ModelScope',
  huggingface: 'Hugging Face',
  csghub: 'CSG Hub',
  openmind_hub: 'OpenMind Hub',
};

/** 采样算法 */
export const SAMPLING_METHODS = [
  'default',
  'DPM++ 2M',
  'DPM++ 2M Karras',
  'DPM++ 2M SDE',
  'DPM++ 2M SDE Karras',
  'DPM++ SDE',
  'DPM++ SDE Karras',
  'DPM2',
  'DPM2 Karras',
  'DPM2 a',
  'DPM2 a Karras',
  'Euler',
  'Euler a',
  'Heun',
  'LMS',
  'LMS Karras',
];
/** 角色 */
export const REPLICA_ROLE_OPTIONS = [
  { label: 'None', value: '' },
  { label: 'Prefill', value: 'P' },
  { label: 'Decode', value: 'D' },
];
