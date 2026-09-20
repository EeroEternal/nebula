import {
  DEFAULT_REPO_LIST_PATH,
  getLastRepoListUrl,
  isRepoListPath,
  resolveRepoMenuPath,
  saveLastRepoListUrl,
} from '../src/utils/repoListUrl';

describe('repoListUrl', () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it('accepts type tabs and rejects downloads / detail', () => {
    expect(isRepoListPath('/models/repository/LLM')).toBe(true);
    expect(isRepoListPath('/models/repository/custom')).toBe(true);
    expect(isRepoListPath('/models/repository/downloads')).toBe(false);
    expect(isRepoListPath('/models/repository/LLM/qwen')).toBe(false);
    expect(isRepoListPath('/models/repository')).toBe(false);
  });

  it('persists path + allowed query', () => {
    saveLastRepoListUrl(
      '/models/repository/embedding',
      '?model_name=bge&curPageNum=2&evil=1',
    );
    expect(getLastRepoListUrl()).toBe(
      '/models/repository/embedding?model_name=bge&curPageNum=2',
    );
  });

  it('first visit has no last url; menu falls back to bare repository', () => {
    expect(getLastRepoListUrl()).toBeNull();
    expect(resolveRepoMenuPath()).toBe('/models/repository');
    expect(DEFAULT_REPO_LIST_PATH).toBe('/models/repository/LLM');
  });

  it('ignores downloads and overlong values', () => {
    saveLastRepoListUrl('/models/repository/downloads', '?curPageNum=2');
    expect(getLastRepoListUrl()).toBeNull();
    saveLastRepoListUrl('/models/repository/LLM', `?model_name=${'x'.repeat(3000)}`);
    expect(getLastRepoListUrl()).toBeNull();
  });
});
