import { transformValueType } from '@/utils';
import { ADETAILER_TAB_INIT_VALUE, CONTROLNET_INIT_VALUE } from '@/constants/sdWebui';

type ADetailerArgs = [boolean, ...Record<string, unknown>[]];

const baseFieldNameMap = {
  Model: 'model',
  Prompt: 'prompt',
  'Negative prompt': 'negative_prompt',
  Sampler: 'sampler_name',
  'Schedule type': 'scheduler',
  Width: 'width',
  Height: 'height',
  Steps: 'steps',
  'CFG scale': 'cfg_scale',
  'Hires upscaler': 'hr_upscaler',
  'Hires steps': 'hr_second_pass_steps',
  'Denoising strength': 'denoising_strength',
  'Hires upscale': 'hr_scale',
  Seed: 'seed',
  'Variation seed': 'subseed',
  'Variation seed strength': 'subseed_strength',
  'Mask blur': 'mask_blur',
};
// 反向映射（显示字段名 → 表单字段名）
const reverseFieldNameMap = Object.fromEntries(
  Object.entries(baseFieldNameMap).map(([k, v]) => [v, k]),
);
const sizeSplitMap = {
  Size: ['width', 'height'],
  'Seed resize from': ['seed_resize_from_w', 'seed_resize_from_h'],
};

const ADetailerFormFieldMap = {
  model: 'ad_model',
  prompt: 'ad_prompt',
  'negative prompt': 'ad_negative_prompt',
  confidence: 'ad_confidence',
  'mask only top k': 'ad_mask_k_largest',
  'mask min ratio': 'ad_mask_min_ratio',
  'mask max ratio': 'ad_mask_max_ratio',
  'x offset': 'ad_x_offset',
  'y offset': 'ad_y_offset',
  'dilate erode': 'ad_dilate_erode',
  'mask merge invert': 'ad_mask_merge_invert',
  'mask blur': 'ad_mask_blur',
  'denoising strength': 'ad_denoising_strength',
  'inpaint only masked': 'ad_inpaint_only_masked',
  'inpaint padding': 'ad_inpaint_only_masked_padding',
  'use inpaint width height': 'ad_use_inpaint_width_height',
  'inpaint width': 'ad_inpaint_width',
  'inpaint height': 'ad_inpaint_height',
  'use separate steps': 'ad_use_steps',
  steps: 'ad_steps',
  'use separate CFG scale': 'ad_use_cfg_scale',
  'CFG scale': 'ad_cfg_scale',
  'use separate VAE': 'ad_use_vae',
  VAE: 'ad_vae',
  'use separate sampler': 'ad_use_sampler',
  sampler: 'ad_sampler',
  scheduler: 'scheduler',
  'use separate noise multiplier': 'ad_use_noise_multiplier',
  'noise multiplier': 'ad_noise_multiplier',
  'use separate CLIP skip': 'ad_use_clip_skip',
  'CLIP skip': 'ad_clip_skip',
  'restore face': 'ad_restore_face',
};

// 去掉开头的'ADetailer '和结尾的' nd'形式的后缀（其中n是数字）
function extractKeyPart(key: string, preKey: string) {
  return key
    .replace(new RegExp(`^${preKey}\\s+`), '')
    .replace(/\s+\d+nd$/, '')
    .trim();
}
/**
 * 将字符串转化为表单对象（ControlNet单条）
 * @param value: "Module: canny, Model: None, Weight: 0.95, Resize Mode: Just Resize, Processor Res: 512, Threshold A: 100.0, Threshold B: 200.0, Guidance Start: 0.05, Guidance End: 0.99, Pixel Perfect: True, Control Mode: ControlNet is more important",
 * @returns { module: 'xxx', resize_mode: 'xxx', ... }
 */
function parseControlNetString(value: string) {
  return value.split(',').reduce((acc, pair) => {
    const [key, value] = pair.split(':').map((s) => s.trim());
    if (!key) return acc;
    const controlNetFormKey = key.replace(/\s+/g, '_').toLowerCase();
    acc[controlNetFormKey] = transformValueType(value);
    return acc;
  }, {});
}
export function parsePromptToObject(rawText: string) {
  // const baseFormField = type === 'img2img' ? :
  const result = {};
  const ADetailerArgs: ADetailerArgs = [true];
  const ControlNetArgs = [];
  let prompt = '';
  let negativePrompt = '';
  let doneWithPrompt = false;

  const lines = rawText.trim().split('\n');

  let lastline = lines.pop() as string;

  const paramRegex = /\s*(\w[\w \-/]+):\s*("(?:\\.|[^\\"]+)"|[^,]*)(?:,|$)/g;
  const paramMatches = [...lastline.matchAll(paramRegex)];
  // 如果最后一行参数不足 3 个，把它当普通文本继续加回 prompt/negative
  if (paramMatches.length < 3) {
    lines.push(lastline);
    lastline = '';
  }
  // 处理提示词/反向提示词
  for (const lineRaw of lines) {
    const line = lineRaw.trim();
    if (line.startsWith('Negative prompt:')) {
      doneWithPrompt = true;
      negativePrompt += (negativePrompt ? '\n' : '') + line.slice(16).trim();
    } else if (doneWithPrompt) {
      negativePrompt += (negativePrompt ? '\n' : '') + line;
    } else {
      prompt += (prompt ? '\n' : '') + line;
    }
  }

  if (prompt) result['prompt'] = prompt;
  if (negativePrompt) result['negative_prompt'] = negativePrompt;
  // 处理 除提示词/反向提示词 的其它字段
  for (const match of lastline.matchAll(paramRegex)) {
    const key = match[1];
    let value = match[2].trim();
    if (value.startsWith('"') && value.endsWith('"')) {
      try {
        value = JSON.parse(value); // unquote
      } catch {
        value = value.slice(1, -1); // fallback
      }
    }
    // 处理 Size 特殊 case
    const sizeMatch = value.match(/^(\d+)x(\d+)$/);
    if (sizeMatch && sizeSplitMap[key]) {
      result[sizeSplitMap[key][0]] = Number(sizeMatch[1]);
      result[sizeSplitMap[key][1]] = Number(sizeMatch[2]);
      continue;
    }
    if (key.startsWith('ADetailer')) {
      // 提取主 key 名 ("ADetailer inpaint only masked 2nd" -> "inpaint only masked")
      const cleanKey = key.replace(/^ADetailer\s+|\s+\d+nd$/g, '')
      if (!ADetailerFormFieldMap[cleanKey]) continue;
      // 提取索引（如果有后缀如 2nd）,不带xnd 的默认为1
      const index = parseInt(key.match(/(\d+)nd$/)?.[1] ?? '1', 10);
      // ADetailer 暂时只有2个单元（后续扩展，可以把此处判断放开）
      if (index > 2) continue;
      // 空对象时赋默认值
      if (typeof ADetailerArgs[index] !== 'object' || ADetailerArgs[index] === null) {
        ADetailerArgs[index] = ADETAILER_TAB_INIT_VALUE;
      }
      ADetailerArgs[index][ADetailerFormFieldMap[cleanKey]] = transformValueType(value);
      continue;
    }
    if (key.startsWith('ControlNet')) {
      if (!value) continue;
      // 提取索引（如果有后缀如 2nd）,不带xnd 的默认为1
      const index = parseInt(key.match(/(\d+)/)?.[1] ?? '0', 10);
      if (index > 3) continue;
      ControlNetArgs[index] = {
        ...CONTROLNET_INIT_VALUE,
        enabled: true,
        ...parseControlNetString(value),
      };
      continue;
    }
    // 基本字段
    const baseFormField = baseFieldNameMap[key];
    if(baseFormField){
      if(baseFormField === 'hr_scale'){
        result['enable_hr'] = true;
      }
      if(baseFormField === 'subseed'){
        result['seedAdvancedSet'] = true;
      }
      result[baseFieldNameMap[key]] = transformValueType(value);
      continue;
    }
  }

  if (ADetailerArgs.length > 1) {
    result['alwayson_scripts'] = {
      ...(result?.['alwayson_scripts'] || {}),
      ADetailer: {
        args: ADetailerArgs,
      },
    };
  }
  if (ControlNetArgs.length > 0) {
    result['alwayson_scripts'] = {
      ...(result?.['alwayson_scripts'] || {}),
      ControlNet: {
        args: ControlNetArgs,
      },
    };
  }
  return result;
}

/**
 * 生成参数转化为生成信息
 */
export function serializeFieldsToPrompt(info: Record<string, unknown>){
  const lines: string[] = [];
  const {
    prompt,
    negative_prompt,
    width,
    height,
    seed_resize_from_w,
    seed_resize_from_h,
    version,
    sd_model_name,
    sd_model_hash,
    ...reset
  } = info;

  if (prompt) lines.push(String(prompt));
  if (negative_prompt) lines.push(`Negative prompt: ${String(negative_prompt)}`);

  const paramParts = [];
  for (const key in reset) {
    if(!reverseFieldNameMap[key] || reset[key] === null) continue;
    paramParts.push(`${reverseFieldNameMap[key]}: ${String(reset[key])}`)
  }
  // Width + Height 合并为 Size
  if (width && height) {
    paramParts.push(`Size: ${String(width)}x${String(height)}`);
  }
  // seed_resize_from_w + seed_resize_from_h 合并为 Seed resize from
  if(seed_resize_from_w && seed_resize_from_h){
    paramParts.push(`Seed resize from: ${String(seed_resize_from_w)}x${String(seed_resize_from_h)}`);
  }
  if(sd_model_hash){
    paramParts.push(`Model hash: ${String(sd_model_hash)}`)
  }
  if(sd_model_name){
    paramParts.push(`Model: ${String(sd_model_name)}`)
  }
  if(version){
    paramParts.push(`Version: ${String(version)}`)
  }
  lines.push(paramParts.join(', '));
  return lines.join('\n');
}