/**
 * PowerLLM Console 管理 API 客户端（全局用户/角色/Release）。
 * 鉴权优先 window.CONSOLE_TOKEN / localStorage.powerllm_console_token，
 * 否则回退到当前登录用户 JWT。
 */

function getConsoleAdminToken(): string {
  try {
    const fromWindow = (window as Window & { CONSOLE_TOKEN?: string }).CONSOLE_TOKEN;
    if (fromWindow) {
      return fromWindow;
    }
    return localStorage.getItem('powerllm_console_token') || '';
  } catch {
    return '';
  }
}

export function consoleHeaders(): Record<string, string> {
  const headers: Record<string, string> = {
    Accept: 'application/json',
    'Content-Type': 'application/json',
  };
  const consoleToken = getConsoleAdminToken();
  if (consoleToken) {
    headers.Authorization = `Bearer ${consoleToken}`;
    return headers;
  }
  try {
    const user = JSON.parse(localStorage.getItem('user') || '{}');
    if (user?.token && user?.token_type) {
      headers.Authorization = `${user.token_type} ${user.token}`;
    }
  } catch {
    // ignore
  }
  return headers;
}

async function parseOrThrow(resp: Response) {
  const data = await resp.json().catch(() => ({}));
  if (!resp.ok) {
    const detail = data?.detail;
    throw new Error(
      typeof detail === 'string' ? detail : detail ? JSON.stringify(detail) : `HTTP ${resp.status}`,
    );
  }
  return data;
}

const base = () => `${window.location.origin}/api/console`;

/** Console 进程可达：JSON 的 200/401/403。SPA/nginx try_files 回 HTML 200 算不可达。 */
export async function probeConsoleReachable(): Promise<boolean> {
  try {
    const resp = await fetch(`${window.location.origin}/api/console/regions/status`, {
      headers: { Accept: 'application/json' },
    });
    if (!(resp.status === 200 || resp.status === 401 || resp.status === 403)) {
      return false;
    }
    const ct = (resp.headers.get('content-type') || '').toLowerCase();
    return ct.includes('application/json');
  } catch {
    return false;
  }
}

export async function fetchConsoleMe() {
  const resp = await fetch(`${base()}/me`, { headers: consoleHeaders() });
  return parseOrThrow(resp);
}

export async function fetchConsoleUsers() {
  const resp = await fetch(`${base()}/users`, { headers: consoleHeaders() });
  const data = await parseOrThrow(resp);
  return data.users || [];
}

export async function createConsoleUser(body: Record<string, unknown>) {
  const resp = await fetch(`${base()}/users`, {
    method: 'POST',
    headers: consoleHeaders(),
    body: JSON.stringify(body),
  });
  return parseOrThrow(resp);
}

export async function updateConsoleUser(username: string, body: Record<string, unknown>) {
  const resp = await fetch(`${base()}/users/${encodeURIComponent(username)}`, {
    method: 'PUT',
    headers: consoleHeaders(),
    body: JSON.stringify(body),
  });
  return parseOrThrow(resp);
}

export async function deleteConsoleUser(username: string) {
  const resp = await fetch(`${base()}/users/${encodeURIComponent(username)}`, {
    method: 'DELETE',
    headers: consoleHeaders(),
  });
  return parseOrThrow(resp);
}

export async function syncConsoleUser(username: string) {
  const resp = await fetch(`${base()}/users/${encodeURIComponent(username)}/sync`, {
    method: 'POST',
    headers: consoleHeaders(),
  });
  return parseOrThrow(resp);
}

export async function fetchConsoleRoles() {
  const resp = await fetch(`${base()}/roles`, { headers: consoleHeaders() });
  const data = await parseOrThrow(resp);
  return data.roles || [];
}

export async function createConsoleRole(body: Record<string, unknown>) {
  const resp = await fetch(`${base()}/roles`, {
    method: 'POST',
    headers: consoleHeaders(),
    body: JSON.stringify(body),
  });
  return parseOrThrow(resp);
}

export async function updateConsoleRole(name: string, body: Record<string, unknown>) {
  const resp = await fetch(`${base()}/roles/${encodeURIComponent(name)}`, {
    method: 'PUT',
    headers: consoleHeaders(),
    body: JSON.stringify(body),
  });
  return parseOrThrow(resp);
}

export async function deleteConsoleRole(name: string) {
  const resp = await fetch(`${base()}/roles/${encodeURIComponent(name)}`, {
    method: 'DELETE',
    headers: consoleHeaders(),
  });
  return parseOrThrow(resp);
}

export async function syncConsoleRole(name: string) {
  const resp = await fetch(`${base()}/roles/${encodeURIComponent(name)}/sync`, {
    method: 'POST',
    headers: consoleHeaders(),
  });
  return parseOrThrow(resp);
}

export async function fetchConsoleReleases() {
  const resp = await fetch(`${base()}/releases`, { headers: consoleHeaders() });
  const data = await parseOrThrow(resp);
  return data.releases || [];
}

export async function createConsoleRelease(body: Record<string, unknown>) {
  const resp = await fetch(`${base()}/releases`, {
    method: 'POST',
    headers: consoleHeaders(),
    body: JSON.stringify(body),
  });
  return parseOrThrow(resp);
}

export async function applyConsoleRelease(id: number, region?: string) {
  const qs = region ? `?region=${encodeURIComponent(region)}` : '';
  const resp = await fetch(`${base()}/releases/${id}/apply${qs}`, {
    method: 'POST',
    headers: consoleHeaders(),
  });
  return parseOrThrow(resp);
}
