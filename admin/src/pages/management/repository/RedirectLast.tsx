import { Navigate } from '@umijs/max';

import { DEFAULT_REPO_LIST_PATH, getLastRepoListUrl } from '@/utils/repoListUrl';

/** 裸 /models/repository：回上次列表（含 query），没有则 LLM */
const RedirectLast = () => (
  <Navigate to={getLastRepoListUrl() || DEFAULT_REPO_LIST_PATH} replace />
);

export default RedirectLast;
