/** Browser-side CSV download helper for monitoring export buttons. */

function escapeCsvCell(value: unknown): string {
  const raw = value === undefined || value === null ? '' : String(value);
  if (/[",\n\r]/.test(raw)) {
    return `"${raw.replace(/"/g, '""')}"`;
  }
  return raw;
}

export function downloadCsv(
  filename: string,
  headers: string[],
  rows: Array<Array<unknown> | Record<string, unknown>>,
): void {
  const lines = [
    headers.map(escapeCsvCell).join(','),
    ...rows.map((row) => {
      if (Array.isArray(row)) {
        return row.map(escapeCsvCell).join(',');
      }
      return headers.map((h) => escapeCsvCell(row[h])).join(',');
    }),
  ];
  const blob = new Blob([`\uFEFF${lines.join('\n')}`], {
    type: 'text/csv;charset=utf-8;',
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename.endsWith('.csv') ? filename : `${filename}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

export function downloadText(filename: string, content: string, mime = 'text/plain'): void {
  const blob = new Blob([content], { type: `${mime};charset=utf-8;` });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
