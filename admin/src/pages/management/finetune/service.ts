import request from '@/utils/request';
import { message } from 'antd';
import { getLocal } from '@/utils';

type FetchOptionType = Omit<RequestInit, 'body'> & {
  params?: Record<string, unknown>;
  body?: BodyInit | Record<string, unknown> | null;
};
type FinetuneTaskId = { task_id: string };
type FinetuneCreatePayload = {
  model_args?: { model_type?: string };
  [key: string]: unknown;
};
type FinetuneTaskPayload = FinetuneCreatePayload & FinetuneTaskId;
type FinetuneListParams = {
  model_type: string;
  curPageNum?: number;
  detailed?: boolean;
  contentLength?: unknown;
  langData?: unknown;
  model_name?: string;
  typeData?: unknown;
  status?: unknown;
  is_builtin?: boolean;
};
/**
 * 获取模型名称
 * @returns {}
 */
export async function getTypeList(params: { model_type: string; is_builtin?: boolean }) {
  return request(`/model_registrations/${params.model_type}`, {
    method: 'GET',
    params: {
      is_builtin: params.is_builtin,
    },
  });
}

/**
 * 更新微调任务
 * @returns {}
 */
export async function getFinetuneDataset(params: Record<string, unknown>) {
  return request(`/tasks/tuning/dataset`, {
    method: 'GET',
    params: params,
  });
}

/**
 * 获取微调任务列表
 * @returns {}
 */
export async function getFinetuneTaskList(params: Record<string, unknown>) {
  return request('/tasks', {
    method: 'GET',
    params: params,
  });
}
/**
 * 获取微调任务详情
 * @returns {}
 */
export async function getFinetuneTaskDetail(params: FinetuneTaskId) {
  return request(`/tasks/${params.task_id}`, {
    method: 'GET',
  });
}
/**
 * 获取微调任务日志
 * @returns {}
 */
export async function getFinetuneTaskLog(params: FinetuneTaskId) {
  return request(`/tasks/${params.task_id}/log`, {
    method: 'GET',
  });
}
/**
 * 获取模型版本信息
 * @returns {}
 */
export async function getVersionList(params: { type: string; name: string }) {
  // const pageparams = {
  //   curPageNum: -1,
  //   numPerPage: -1,
  // };
  return request(`/models/${params.type}/${params.name}/versions`, {
    method: 'GET',
    // params: pageparams,
  });
}
/**
 * 创建微调任务
 * @returns {}
 */
export async function createFinetuneTask(params: FinetuneCreatePayload) {
  return request(`/tasks/tuning/${params.model_args?.model_type}`, {
    method: 'POST',
    data: params,
  });
}
/**
 * 更新微调任务
 * @returns {}
 */
export async function updateFinetuneTask(params: FinetuneTaskPayload) {
  return request(`/tasks/${params.task_id}/modify`, {
    method: 'PUT',
    data: params,
  });
}
/**
 * 删除微调任务
 * @returns {}
 */
export async function deleteFinetuneTask(params: FinetuneTaskId) {
  return request(`/tasks/${params.task_id}`, {
    method: 'DELETE',
  });
}
/**
 * 一键启动微调任务
 * @returns {}
 */
export async function launchFinetuneTask(params: FinetuneTaskId) {
  return request(`/tasks/${params.task_id}/start`, {
    method: 'POST',
  });
}

// 获取模型列表
export async function getList(params: FinetuneListParams) {
  return request(`/model_registrations/${params.model_type}`, {
    method: 'GET',
    params: {
      curPageNum: params.curPageNum || 1,
      numPerPage: 10,
      detailed: params.detailed ?? true,
      context_length: params.contentLength,
      model_lang: params.langData,
      model_name: params.model_name || undefined,
      model_ability: params.typeData,
      cache_status: params.status,
      is_builtin: params.is_builtin ?? true,
    },
  });
}
const ContentType = {
  json: 'application/json',
  stream: 'text/event-stream',
  form: 'application/x-www-form-urlencoded; charset=UTF-8',
  download: 'application/octet-stream', // for download
  upload: 'multipart/form-data', // for upload
};
export type MessageReplace = {
  id: string;
  task_id: string;
  answer: string;
  conversation_id: string;
};

export type AnnotationReply = {
  id: string;
  task_id: string;
  answer: string;
  conversation_id: string;
  annotation_id: string;
  annotation_author_name: string;
};
export type WorkflowStartedResponse = {
  task_id: string;
  workflow_run_id: string;
  event: string;
  data: {
    id: string;
    workflow_id: string;
    sequence_number: number;
    created_at: number;
  };
};

export type WorkflowFinishedResponse = {
  task_id: string;
  workflow_run_id: string;
  event: string;
  data: {
    id: string;
    workflow_id: string;
    status: string;
    outputs: unknown;
    error: string;
    elapsed_time: number;
    total_tokens: number;
    total_steps: number;
    created_at: number;
    finished_at: number;
  };
};

export type NodeStartedResponse = {
  task_id: string;
  workflow_run_id: string;
  event: string;
  data: {
    id: string;
    node_id: string;
    node_type: string;
    index: number;
    predecessor_node_id?: string;
    inputs: unknown;
    created_at: number;
    extras?: unknown;
  };
};

export type NodeFinishedResponse = {
  task_id: string;
  workflow_run_id: string;
  event: string;
  data: {
    id: string;
    node_id: string;
    node_type: string;
    index: number;
    predecessor_node_id?: string;
    inputs: unknown;
    process_data: unknown;
    outputs: unknown;
    status: string;
    error: string;
    elapsed_time: number;
    execution_metadata: {
      total_tokens: number;
      total_price: number;
      currency: string;
    };
    created_at: number;
  };
};

export type TextChunkResponse = {
  task_id: string;
  workflow_run_id: string;
  event: string;
  data: {
    text: string;
  };
};

export type TextReplaceResponse = {
  task_id: string;
  workflow_run_id: string;
  event: string;
  data: {
    text: string;
  };
};
export type CitationItem = {
  content: string;
  data_source_type: string;
  dataset_name: string;
  dataset_id: string;
  document_id: string;
  document_name: string;
  hit_count: number;
  index_node_hash: string;
  segment_id: string;
  segment_position: number;
  score: number;
  word_count: number;
};
export type MessageEnd = {
  id: string;
  metadata: {
    retriever_resources?: CitationItem[];
    annotation_reply: {
      id: string;
      account: {
        id: string;
        name: string;
      };
    };
  };
};

export enum TransferMethod {
  all = 'all',
  local_file = 'local_file',
  remote_url = 'remote_url',
}
export type VisionFile = {
  id?: string;
  type: string;
  transfer_method: TransferMethod;
  url: string;
  upload_file_id: string;
  belongs_to?: string;
};
export type ThoughtItem = {
  id: string;
  tool: string; // plugin or dataset. May has multi.
  thought: string;
  tool_input: string;
  message_id: string;
  observation: string;
  position: number;
  files?: string[];
  message_files?: VisionFile[];
};
export type IOnDataMoreInfo = {
  conversationId?: string;
  taskId?: string;
  messageId?: string;
  errorMessage?: string;
  errorCode?: string;
};

export type IOnData = (message: string, isFirstMessage: boolean, moreInfo: IOnDataMoreInfo) => void;
export type IOnThought = (though: ThoughtItem) => void;
export type IOnFile = (file: VisionFile) => void;
export type IOnMessageEnd = (messageEnd: MessageEnd) => void;
export type IOnMessageReplace = (messageReplace: MessageReplace) => void;
export type IOnAnnotationReply = (messageReplace: AnnotationReply) => void;
export type IOnCompleted = (hasError?: boolean, errorMessage?: string) => void;
export type IOnError = (msg: string, code?: string) => void;

export type IOnWorkflowStarted = (workflowStarted: WorkflowStartedResponse) => void;
export type IOnWorkflowFinished = (workflowFinished: WorkflowFinishedResponse) => void;
export type IOnNodeStarted = (nodeStarted: NodeStartedResponse) => void;
export type IOnNodeFinished = (nodeFinished: NodeFinishedResponse) => void;
export type IOnTextChunk = (textChunk: TextChunkResponse) => void;
export type IOnTextReplace = (textReplace: TextReplaceResponse) => void;
const handleStream = (
  response: Response,
  onData: IOnData,
  onCompleted?: IOnCompleted,
  _onThought?: IOnThought,
  _onMessageEnd?: IOnMessageEnd,
  _onMessageReplace?: IOnMessageReplace,
  _onFile?: IOnFile,
  _onWorkflowStarted?: IOnWorkflowStarted,
  _onWorkflowFinished?: IOnWorkflowFinished,
  _onNodeStarted?: IOnNodeStarted,
  _onNodeFinished?: IOnNodeFinished,
  _onTextChunk?: IOnTextChunk,
  _onTextReplace?: IOnTextReplace,
) => {
  if (!response.ok) throw new Error('Network response was not ok');

  const reader = response.body?.getReader();
  const decoder = new TextDecoder('utf-8');
  let buffer = '';
  let isFirstMessage = true;
  function read() {
    let hasError = false;
    reader?.read().then((result: ReadableStreamReadResult<Uint8Array>) => {
      if (result.done) {
        onCompleted && onCompleted();
        return;
      }
      buffer += decoder.decode(result.value, { stream: true });
      const lines = buffer.split('\n');
      try {
        lines.forEach((message) => {
          if (message.startsWith('data: ')) {
            onData(message.substring(6), isFirstMessage, {});
            isFirstMessage = false;
          }
        });
        buffer = lines[lines.length - 1];
      } catch (e) {
        onData('', false, {
          conversationId: undefined,
          messageId: '',
          errorMessage: `${e}`,
        });
        hasError = true;
        onCompleted?.(true, e as string);
        return;
      }
      if (!hasError) read();
    });
  }
  read();
};

export const ssePost = (
  url: string,
  fetchOptions: FetchOptionType,
  {
    isPublicAPI = false,
    onData,
    onCompleted,
    onThought,
    onFile,
    onMessageEnd,
    onMessageReplace,
    onWorkflowStarted,
    onWorkflowFinished,
    onNodeStarted,
    onNodeFinished,
    onTextChunk,
    onTextReplace,
    onError,
    getAbortController,
  }: {
    isPublicAPI?: boolean;
    onData?: IOnData;
    onCompleted?: IOnCompleted;
    onThought?: IOnThought;
    onFile?: IOnFile;
    onMessageEnd?: IOnMessageEnd;
    onMessageReplace?: IOnMessageReplace;
    onWorkflowStarted?: IOnWorkflowStarted;
    onWorkflowFinished?: IOnWorkflowFinished;
    onNodeStarted?: IOnNodeStarted;
    onNodeFinished?: IOnNodeFinished;
    onTextChunk?: IOnTextChunk;
    onTextReplace?: IOnTextReplace;
    onError?: IOnError;
    getAbortController?: (controller: AbortController) => void;
  },
) => {
  // 创建一个 AbortController 实例
  const abortController = new AbortController();
  const { token, token_type: type } = getLocal('user') || {};
  const options = Object.assign(
    {},
    {
      method: 'GET',
      //mode: 'no-cors',
      //credentials: 'include', // always send cookies、HTTP Basic authentication.
      headers: new Headers({
        'Content-Type': ContentType.json,
        Authorization: `${type} ${token}`,
      }),
      redirect: 'follow',
    },
    {
      method: 'GET',
      signal: abortController.signal, // 获取该控制器的信号
    },
    fetchOptions,
  );

  const contentType = options.headers.get('Content-Type');
  if (!contentType) options.headers.set('Content-Type', ContentType.json);

  getAbortController?.(abortController);

  const urlPrefix = window.DOMAIN_API;
  const urlWithPrefix = `${urlPrefix}${url.startsWith('/') ? url : `/${url}`}`;

  const { body } = options;
  if (body) options.body = JSON.stringify(body);

  fetch(urlWithPrefix, options as RequestInit)
    .then((res) => {

      if (!/^(2|3)\d{2}$/.test(String(res.status))) {
        res.json().then((data: { message?: string }) => {
          message.error(data.message || 'Server Error');
        });
        onError?.('Server Error');
        return;
      }
      return handleStream(
        res,
        (str: string, isFirstMessage: boolean, moreInfo: IOnDataMoreInfo) => {
          if (moreInfo.errorMessage) {
            onError?.(moreInfo.errorMessage, moreInfo.errorCode);
            if (moreInfo.errorMessage !== 'AbortError: The user aborted a request.')
              message.error(moreInfo.errorMessage);
            return;
          }
          onData?.(str, isFirstMessage, moreInfo);
        },
        onCompleted,
        onThought,
        onMessageEnd,
        onMessageReplace,
        onFile,
        onWorkflowStarted,
        onWorkflowFinished,
        onNodeStarted,
        onNodeFinished,
        onTextChunk,
        onTextReplace,
      );
    })
    .catch((e) => {


      if (e.toString() !== 'AbortError: The user aborted a request.') message.error(e);
      onError?.(e);
    });
};
