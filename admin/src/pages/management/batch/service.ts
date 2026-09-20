import request from '@/utils/request';
import { message } from 'antd';
import { getLocal } from '@/utils';

type FetchOptionType = Omit<RequestInit, 'body'> & {
  params?: Record<string, unknown>;
  body?: BodyInit | Record<string, unknown> | null;
};

type BatchIdParams = { id: string | number };
type BatchListParams = { curPageNum?: number; status?: string };

/**
 * 检索批次
 * @returns {}
 */
export async function getBatchDetail(params: BatchIdParams) {
  return request(`/batches/${params.id}`, {
    method: 'GET',
  });
}

/**
 * 取消批次
 * @returns {}
 */
export async function cancelBatch(params: BatchIdParams) {
  return request(`/batches/${params.id}`, {
    method: 'POST',
  });
}
/**
 * 删除批次
 * @returns {}
 */
export async function deleteBatch(params: BatchIdParams) {
  return request(`/batches/${params.id}`, {
    method: 'DELETE',
  });
}
// 列出批次
export async function getBatchList(params: BatchListParams) {
  return request(`/batches`, {
    method: 'GET',
    params: {
      cur_page_num: params.curPageNum || 1,
      num_per_page: 10,
      status: params.status || undefined,
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
