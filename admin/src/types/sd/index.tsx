export interface SDModelsItem {
  model_name: string;
  config: null;
}
export interface SamplersItem {
  name: string;
  alias: string[];
  options: Record<string, unknown>;
}
export interface LorasItem {
  alias: string;
  metadata: {
    ss_output_name: string;
  };
  name: string;
  path: string | null;
}
export interface UpScalersItem {
  model_name: string;
  model_path: string;
  model_url: string;
  name: string;
  scale: 4
}

export interface ControlTypeItem {
  default_model: string;
  default_option: string;
  model_list: string[];
  module_list: string[];
}
export type ControlTypes = Record<string, ControlTypeItem>