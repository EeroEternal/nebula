import { lGet } from '@/utils/intl';

export const CLIP = {
  min: 1,
  max: 12,
  defaultValue: 2,
};

export const INIT_SAMPLER = 'Euler a';

export const SCHEDULER_OPTIONS = [
  {
    label: lGet('management.drawingTool.adetailer.inpainting.scheduler.automatic'),
    value: 'Automatic',
  },
  { label: 'Uniform', value: 'Uniform' },
  { label: 'Karras', value: 'Karras' },
  { label: 'Exponential', value: 'Exponential' },
  { label: 'Polyexponential', value: 'Polyexponential' },
  { label: 'SGM Uniform', value: 'SGM Uniform' },
  { label: 'KL Optimal', value: 'KL Optimal' },
  { label: 'Align Your Steps', value: 'Align Your Steps' },
  { label: 'Simple', value: 'Simple' },
  { label: lGet('management.drawingTool.adetailer.inpainting.scheduler.normal'), value: 'Normal' },
  { label: 'DDIM', value: 'DDIM' },
  { label: 'Beta', value: 'Beta' },
];
/** 迭代步数 */
export const STEPS = {
  min: 1,
  max: 60,
  defaultValue: 20,
};

/** 图片宽高 */
export const IMAGE_SIZE = {
  min: 128,
  max: 1024,
  defaultValue: 512,
};

/** 提示词引导系数 */
export const CFG_SCALE = {
  min: 1,
  max: 30,
  defaultValue: 7,
  step: 0.5,
  precision: 1,
};

/** 种子来源 */
export const SEED_FROM_OPTIONS = [
  { label: 'CPU', value: 'CPU' },
  { label: 'GPU', value: 'GPU' },
];
/** 随机数种子 */
export const SEED_MIN = -1;
/** 随机数种子默认值 */
export const SEED_VAlUE = -1;
/** 差异随机种子 */
export const SUB_SEED_VALUE = -1;
/** 变异强度 */
export const SUBSEED_STRENGTH = {
  min: 0,
  max: 1,
  defaultValue: 0,
  precision: 2,
  step: 0.01,
};
/** 从 宽/高 度中调整种子 */
export const SEED_RESIZE_FROM = {
  min: 0,
  max: 1024,
  defaultValue: 0,
  step: 8,
};

/** 重绘幅度 */
export const DENOISING_STRENGTH = {
  min: 0,
  max: 1,
  defaultValue: 0.75,
  precision: 2,
  step: 0.01,
};
/** 图片数量 */
export const BATCH_SIZE_OPTIONS = [1, 2, 3, 4, 8, 16];

/** 迭代次数 */
export const N_ITER = {
  min: 1,
  max: 10,
  defaultValue: 1,
};
export const DEFAULT_HR_UPSCALER_OPTIONS = [
  'None',
  'Latent',
  'Latent (antialiased)',
  'Latent (bicubic)',
  'Latent (bicubic antialiased)',
  'Latent (nearest)',
  'Latent (nearest-exact)',
  'Lanczos',
  'Nearest',
  'R-ESRGAN 4x+',
  'R-ESRGAN 4x+ Anime6B',
  'LDSR',
  'ScuNET GAN',
  'ScuNET PSNR',
  'SwinIR 4x',
];
/** 高分辨率修复-缩放方式 */
export const HR_SCALE_METHOD = [
  { label: lGet('management.drawingTool.proportionalScale'), value: 'proportionalScale' },
  { label: lGet('management.drawingTool.specifiedResolution'), value: 'specifiedResolution' },
];
/** 高分辨率修复-重绘采样步数 */
export const HR_SECOND_PASS_STEPS = {
  min: 1,
  max: 30,
  defaultValue: 20,
};

/** 高分辨率修复-等比缩放-放大倍率 */
export const HR_SCALE = {
  min: 1,
  max: 4,
  defaultValue: 2,
  step: 0.05,
  precision: 2,
};
/** 高分辨率修复-按指定分辨率缩放- 默认值 */
export const HR_RESIZE_VALUE = IMAGE_SIZE.defaultValue * HR_SCALE.defaultValue;

/** 蒙版模糊程度 */
export const MASK_BLUR = {
  min: 0,
  max: 64,
  defaultValue: 4,
};
/** 填充像素数 */
export const INPAINT_FULL_RES_PADDING = {
  min: 0,
  max: 256,
  defaultValue: 32,
};
/** ADetailer-模型 */
export const ADETAILER_MODEL_OPTIONS = [
  { label: 'face_yolov8n', value: 'face_yolov8n.pt' },
  { label: 'face_yolov8s', value: 'face_yolov8s.pt' },
  { label: 'hand_yolov8n', value: 'hand_yolov8n.pt' },
];

/** ADetailer-目标检测-目标检测阈值 */
export const AD_CONFIDENCE = {
  min: 0,
  max: 1,
  defaultValue: 0.3,
  step: 0.01,
  precision: 2,
};
/** ADetailer-目标检测-目标检测阈值 */
export const AD_MASK_K_LARGEST = {
  min: 0,
  max: 10,
  defaultValue: 0,
};
/** ADetailer-目标检测-最小区域蒙版比例 */
export const AD_MASK_MIN_RATIO = {
  min: 0,
  max: 1,
  defaultValue: 0,
  step: 0.01,
  precision: 2,
};
/** ADetailer-目标检测-最大区域蒙版比例 */
export const AD_MASK_MAX_RATIO = {
  min: 0,
  max: 1,
  defaultValue: 1,
  step: 0.01,
  precision: 2,
};
/** ADetailer-蒙版的预处理-蒙版 X/Y 轴位移 */
export const AD_MASK_OFFSET = {
  min: -200,
  max: 200,
  defaultValue: 0,
};

/** ADetailer-蒙版的预处理- 蒙版 羽化(-)/锐化(+) */
export const AD_DILATE_ERODE = {
  min: -128,
  max: 128,
  defaultValue: 4,
  step: 4,
};

/** ADetailer-蒙版的预处理-合并模式 */
export const AD_MASK_MERGE_INVERT_OPTIONS = [
  {
    label: lGet('management.drawingTool.adetailer.maskPreprocessing.mergeMode.none'),
    value: 'None',
  },
  {
    label: lGet('management.drawingTool.adetailer.maskPreprocessing.mergeMode.merge'),
    value: 'Merge',
  },
  {
    label: lGet('management.drawingTool.adetailer.maskPreprocessing.mergeMode.mergeAndInvert'),
    value: 'Merge and Invert',
  },
];

/** ADetailer-局部重绘-重绘幅度 */
export const AD_DENOISING_STRENGTH = {
  min: 0,
  max: 1,
  defaultValue: 0.4,
  step: 0.01,
  precision: 2,
};

/** ADetailer-局部重绘-仅蒙版模式的边缘预留像素 */
export const AD_ONLY_MASKED_PADDING = {
  min: 0,
  max: 256,
  defaultValue: 32,
  step: 4,
};
/** ADetailer-局部重绘- 重设画布尺寸宽/高  */
export const AD_INPAINT_WH = {
  min: 64,
  max: 2048,
  defaultValue: 512,
  step: 4,
};
/** ADetailer-局部重绘-ADetailer 迭代步数 */
export const AD_INPAINTING_STEPS = {
  min: 1,
  max: 150,
  defaultValue: 28,
};

/** ADetailer-局部重绘-提示词相关性 */
export const AD_CFG_SCALE = {
  min: 0,
  max: 30,
  defaultValue: 7,
  step: 0.5,
  precision: 1,
};
/** ADetailer-局部重绘-VAE */
export const AD_USE_VAE_OPTIONS = [
  {
    label: lGet('management.drawingTool.adetailer.inpainting.Vae.useSameVae'),
    value: 'Use same VAE',
  },
  { label: lGet('management.drawingTool.adetailer.inpainting.Vae.automatic'), value: 'Automatic' },
  { label: lGet('management.drawingTool.adetailer.inpainting.Vae.none'), value: 'None' },
];
/** ADetailer-局部重绘-采样器 */
export const AD_INIT_SAMPLER = 'DPM++ 2M Karras';

/** ADetailer-局部重绘-噪音系数 */
export const AD_NOISE_MULTIPLIER = {
  min: 0.5,
  max: 1.5,
  defaultValue: 1,
  step: 0.01,
  precision: 2,
};

/** ADetailer-局部重绘- CLIP SKIP */
export const AD_CLIP_SKIP = {
  min: 1,
  max: 12,
  defaultValue: 1,
};

/** ADetailer-重设迭代步数 */
export const AD_STEPS = {
  min: 1,
  max: 60,
  defaultValue: 20,
};

/** ControlNet-控制权重*/
export const CONTROL_WEIGHT = {
  min: 0,
  max: 2,
  defaultValue: 1,
  precision: 2,
  step: 0.05,
};
/** ControlNet-起始步数 */
export const CONTROL_GUIDANCE_START = {
  min: 0,
  max: 1,
  defaultValue: 0,
  precision: 2,
  step: 0.01,
};
/** ControlNet-完结步数 */
export const CONTROL_GUIDANCE_END = {
  min: 0,
  max: 1,
  defaultValue: 1,
  precision: 2,
  step: 0.01,
};
/** ControlNet-预处理器分辨率 */
export const CONTROL_PREPROCESSOR_RESOLUTION = {
  min: 64,
  max: 2048,
  defaultValue: 512,
};
/** ControlNet-Threshold */
export const CONTROL_THRESHOLD_MAP = {
  canny: {
    low: {
      min: 1,
      max: 255,
      value: 100,
      label: 'Canny Low Threshold',
    },
    high: {
      min: 1,
      max: 255,
      value: 200,
      label: 'Canny High Threshold',
    },
  },
};
/** 需要展示Threshold的 keys */
export const SHOW_THRESHOLD = Object.keys(CONTROL_THRESHOLD_MAP);
/**  */
export const HR_OPTION = [
  { label: lGet('management.drawingTool.controlNet.hrOption.both'), value: 'Both' },
  { label: lGet('management.drawingTool.controlNet.hrOption.low'), value: 'Low res only' },
  { label: lGet('management.drawingTool.controlNet.hrOption.high'), value: 'High res only' },
];
/** ControlNet-控制模式 */
export const CONTROL_MODE = [
  { label: lGet('management.drawingTool.controlNet.controlMode.balanced'), value: 'Balanced' },
  {
    label: lGet('management.drawingTool.controlNet.controlMode.prompt'),
    value: 'My prompt is more important',
  },
  {
    label: lGet('management.drawingTool.controlNet.controlMode.control'),
    value: 'ControlNet is more important',
  },
];
/** 图片缩放模式 */
export const RESIZE_MODE_STR = [
  { label: lGet('management.drawingTool.controlNet.resizeMode.justResize'), value: 'Just Resize' },
  {
    label: lGet('management.drawingTool.controlNet.resizeMode.innerFit'),
    value: 'Crop and Resize',
  },
  {
    label: lGet('management.drawingTool.controlNet.resizeMode.outerFit'),
    value: 'Resize and Fill',
  },
];
/** 图片缩放模式 */
export const RESIZE_MODE_INT = [
  { label: lGet('management.drawingTool.controlNet.resizeMode.justResize'), value: 0 },
  {
    label: lGet('management.drawingTool.controlNet.resizeMode.innerFit'),
    value: 1,
  },
  {
    label: lGet('management.drawingTool.controlNet.resizeMode.outerFit'),
    value: 2,
  },
];
/** 是否反转蒙版 */
export const INPAINTING_MASK_INVERT = [
  { label: lGet('management.drawingTool.inpaintingMaskInvert.white'), value: 0 },
  { label: lGet('management.drawingTool.inpaintingMaskInvert.black'), value: 1 },
];

export const ADETAILER_TAB_INIT_VALUE = {
  ad_confidence: AD_CONFIDENCE.defaultValue,
  ad_mask_k_largest: AD_MASK_K_LARGEST.defaultValue,
  ad_mask_min_ratio: AD_MASK_MIN_RATIO.defaultValue,
  ad_mask_max_ratio: AD_MASK_MAX_RATIO.defaultValue,
  ad_x_offset: AD_MASK_OFFSET.defaultValue,
  ad_y_offset: AD_MASK_OFFSET.defaultValue,
  ad_dilate_erode: AD_DILATE_ERODE.defaultValue,
  ad_mask_merge_invert: AD_MASK_MERGE_INVERT_OPTIONS[0].value,
  ad_mask_blur: MASK_BLUR.defaultValue,
  ad_denoising_strength: AD_DENOISING_STRENGTH.defaultValue,
  ad_inpaint_only_masked: true,
  ad_inpaint_only_masked_padding: AD_ONLY_MASKED_PADDING.defaultValue,
  ad_use_inpaint_width_height: false,
  ad_inpaint_width: AD_INPAINT_WH.defaultValue,
  ad_inpaint_height: AD_INPAINT_WH.defaultValue,
  ad_use_steps: false,
  ad_steps: AD_INPAINTING_STEPS.defaultValue,
  ad_use_cfg_scale: false,
  ad_cfg_scale: AD_CFG_SCALE.defaultValue,
  ad_use_vae: false,
  ad_vae: AD_USE_VAE_OPTIONS[0].value,
  ad_use_sampler: false,
  ad_sampler: AD_INIT_SAMPLER,
  ad_use_noise_multiplier: false,
  ad_noise_multiplier: AD_NOISE_MULTIPLIER.defaultValue,
  ad_use_clip_skip: false,
  ad_clip_skip: AD_CLIP_SKIP.defaultValue,
  ad_restore_face: false,
};
export const CONTROLNET_INIT_VALUE = {
  enabled: false,
  pixel_perfect: false,
  weight: CONTROL_WEIGHT.defaultValue,
  guidance_start: CONTROL_GUIDANCE_START.defaultValue,
  guidance_end: CONTROL_GUIDANCE_END.defaultValue,
  processor_res: CONTROL_PREPROCESSOR_RESOLUTION.defaultValue,
  low_vram: false,
  hr_option: HR_OPTION[0].value,
  control_mode: CONTROL_MODE[0].value,
  resize_mode: RESIZE_MODE_STR[1].value,
};
export type ControlNetField =  typeof CONTROLNET_INIT_VALUE;
export const TXT_2_IMG_INIT_VALUE = {
  override_settings: {
    clip_skip: CLIP.defaultValue,
  },
  prompt: '',
  negative_prompt: '',
  sampler_name: INIT_SAMPLER,
  scheduler: SCHEDULER_OPTIONS[0].value,
  batch_size: BATCH_SIZE_OPTIONS[0],
  n_iter: N_ITER.defaultValue,
  width: IMAGE_SIZE.defaultValue,
  height: IMAGE_SIZE.defaultValue,
  steps: STEPS.defaultValue,
  cfg_scale: CFG_SCALE.defaultValue,
  enable_hr: false,
  seed: SEED_VAlUE,
  seedAdvancedSet: false,
  alwayson_scripts: {
    ADetailer: {
      args: [false, ADETAILER_TAB_INIT_VALUE, ADETAILER_TAB_INIT_VALUE],
    },
    ControlNet: {
      args: [
        CONTROLNET_INIT_VALUE,
        CONTROLNET_INIT_VALUE,
        CONTROLNET_INIT_VALUE,
        CONTROLNET_INIT_VALUE,
      ],
    },
  },
};
export const IMG_2_IMG_INIT_VALUE = {
  override_settings: {
    clip_skip: CLIP.defaultValue,
  },
  resize_mode: RESIZE_MODE_INT[0].value,
  inpainting_mask_invert: INPAINTING_MASK_INVERT[0].value,
  mask_blur: MASK_BLUR.defaultValue,
  sampler_name: INIT_SAMPLER,
  scheduler: SCHEDULER_OPTIONS[0].value,
  steps: STEPS.defaultValue,
  batch_size: BATCH_SIZE_OPTIONS[0],
  n_iter: N_ITER.defaultValue,
  width: IMAGE_SIZE.defaultValue,
  height: IMAGE_SIZE.defaultValue,
  cfg_scale: CFG_SCALE.defaultValue,
  denoising_strength: DENOISING_STRENGTH.defaultValue,
  inpaint_full_res: false,
  inpaint_full_res_padding: INPAINT_FULL_RES_PADDING.defaultValue,
  seed: SEED_VAlUE,
  seedAdvancedSet: false,
  alwayson_scripts: {
    ADetailer: {
      args: [false, ADETAILER_TAB_INIT_VALUE],
    },
    ControlNet: {
      args: [CONTROLNET_INIT_VALUE],
    },
  },
};

