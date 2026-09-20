/** 能力试玩表单 / 结果包络（#75 类型债，不约束后端字段全集） */

export type AbilityUploadFile = { originFileObj: Blob };

export type AbilityFormValues = {
  model?: string;
  n?: string | number;
  prompt?: string;
  input?: string;
  query?: string;
  replica_id?: string;
  language?: string;
  file?: AbilityUploadFile[] | Blob | string;
  prompt_speech?: Blob | string;
  inpainting_image?: Blob | string;
  mask_image?: Blob | string;
  image?: AbilityUploadFile[];
  first_frame?: AbilityUploadFile[];
  last_frame?: AbilityUploadFile[];
  documents?: { corpus: string }[];
  kwargs?: Record<string, unknown>;
  [key: string]: unknown;
};

export type AbilityImageItem = { url?: string; b64_json?: string };

export type AbilityUsage = {
  total_tokens?: number;
  prompt_tokens?: number;
  completion_tokens?: number;
};

export type AbilityRunResult = {
  success?: boolean;
  data?: unknown;
};

export type AbilityApiResult = {
  data?: AbilityImageItem[] | Array<{ embedding?: unknown }>;
  choices?: Array<{ text?: string }>;
  results?: unknown;
  usage?: AbilityUsage;
  text?: string;
  created?: number;
  [key: string]: unknown;
};

export function formPart(value: unknown): string | Blob {
  if (value instanceof Blob) return value;
  return value == null ? '' : String(value);
}

export function asImageList(data: unknown): AbilityImageItem[] {
  return Array.isArray(data) ? (data as AbilityImageItem[]) : [];
}

export function asApiResult(res: AbilityApiResult | Blob): AbilityApiResult | undefined {
  return res instanceof Blob ? undefined : res;
}

export function firstUpload(files?: AbilityUploadFile[]): Blob {
  return files?.[0]?.originFileObj ?? new Blob();
}
