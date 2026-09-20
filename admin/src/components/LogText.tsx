import { Fragment, type CSSProperties, type ReactNode } from 'react';

/** 白底日志护眼配色 */
const C = {
  ink: '#1E1E1E',
  muted: '#8E8E93',
  success: '#007A33',
  warn: '#E37400',
  error: '#D0021B',
  accent: '#0B6E99',
  timestamp: '#4A4A4A',
} as const;

const LEVEL_STYLE: Record<string, CSSProperties> = {
  CRITICAL: { color: C.error, fontWeight: 700 },
  ERROR: { color: C.error, fontWeight: 700 },
  WARNING: { color: C.warn, fontWeight: 600 },
  WARN: { color: C.warn, fontWeight: 600 },
  SUCCESS: { color: C.success, fontWeight: 600 },
  OK: { color: C.success, fontWeight: 600 },
  INFO: { color: C.ink, fontWeight: 600 },
  DEBUG: { color: C.muted },
  TRACE: { color: C.muted },
};

const TOKEN_RE =
  /(\[[0-9]{4}-[0-9]{2}-[0-9]{2}[ T][0-9:.]+])|\b(CRITICAL|ERROR|WARNING|WARN|SUCCESS|OK|INFO|DEBUG|TRACE)\b|\b((?:request_id|trace_id|model_uid|replica_model_uid|span_id|job_id)=[^\s,;]+)\b|\b([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\b|("(?:GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)\s[^"]*"\s+)([1-5][0-9]{2})\b/gi;

function detectLevel(line: string): string | undefined {
  const m = line.match(/\b(CRITICAL|ERROR|WARNING|WARN|SUCCESS|OK|INFO|DEBUG|TRACE)\b/i);
  return m?.[1]?.toUpperCase();
}

function styleForMatch(match: RegExpExecArray): CSSProperties {
  const [, ts, level, kv, uuid, httpPrefix, httpStatus] = match;
  if (ts) return { color: C.timestamp };
  if (level) return LEVEL_STYLE[level.toUpperCase()] || { color: C.ink };
  if (kv) return { color: C.accent, fontWeight: 600 };
  if (uuid) return { color: C.accent, fontWeight: 600 };
  if (httpStatus) {
    const code = Number(httpStatus);
    return {
      color: code >= 500 ? C.error : code >= 400 ? C.warn : C.success,
      fontWeight: 600,
    };
  }
  if (httpPrefix) return { color: C.ink };
  return { color: C.ink };
}

/** 单行日志：时间戳 / 级别 / ID / HTTP 状态着色 */
export function highlightLogLine(line: string, keyPrefix = 'l'): ReactNode {
  if (!line) return line;
  const level = detectLevel(line);
  const baseColor =
    level === 'DEBUG' || level === 'TRACE' ? C.muted : C.ink;

  const parts: ReactNode[] = [];
  let last = 0;
  let i = 0;
  const re = new RegExp(TOKEN_RE.source, TOKEN_RE.flags);
  let m: RegExpExecArray | null;
  while ((m = re.exec(line)) !== null) {
    if (m.index > last) {
      parts.push(
        <span key={`${keyPrefix}-${i++}`} style={{ color: baseColor }}>
          {line.slice(last, m.index)}
        </span>,
      );
    }
    // HTTP 匹配拆成 path + status 两组
    if (m[5] != null && m[6] != null) {
      parts.push(
        <span key={`${keyPrefix}-${i++}`} style={{ color: baseColor }}>
          {m[5]}
        </span>,
      );
      parts.push(
        <span key={`${keyPrefix}-${i++}`} style={styleForMatch(m)}>
          {m[6]}
        </span>,
      );
    } else {
      parts.push(
        <span key={`${keyPrefix}-${i++}`} style={styleForMatch(m)}>
          {m[0]}
        </span>,
      );
    }
    last = m.index + m[0].length;
  }
  if (last < line.length) {
    parts.push(
      <span key={`${keyPrefix}-${i++}`} style={{ color: baseColor }}>
        {line.slice(last)}
      </span>,
    );
  }
  return parts.length ? <>{parts}</> : line;
}

type LogTextProps = {
  lines: string[];
  className?: string;
};

/** 多行白底日志（实例 / 服务日志共用） */
const LogText = ({ lines, className }: LogTextProps) => (
  <pre
    className={
      className ||
      'm-0 text-xs font-mono whitespace-pre-wrap break-all leading-5'
    }
  >
    {lines.map((line, idx) => (
      <Fragment key={idx}>
        {idx > 0 ? '\n' : null}
        {highlightLogLine(line, String(idx))}
      </Fragment>
    ))}
  </pre>
);

export default LogText;
