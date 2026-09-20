/** Split l()/lGet()/useL() args: object 2nd param is values, not defaultMessage. */

export function resolveIntlArgs(
  defaultMessage?: string | Record<string, unknown>,
  value?: Record<string, unknown>,
): { defaultMessage?: string; values?: Record<string, unknown> } {
  if (
    defaultMessage != null &&
    typeof defaultMessage === 'object' &&
    !Array.isArray(defaultMessage)
  ) {
    return { values: defaultMessage };
  }
  return {
    defaultMessage: typeof defaultMessage === 'string' ? defaultMessage : undefined,
    values: value,
  };
}
