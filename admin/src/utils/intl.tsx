import { useIntl, getIntl, FormattedMessage } from '@umijs/max';

import { resolveIntlArgs } from './intlArgs';

type IntlValues = Record<string, unknown>;

/**
 * 读当前 locale 文案。不是 hook：内部用 getIntl()，可在 .map() / 分支 / 工具函数里调用。
 * 组件若要订阅 locale 变化，用 useL。
 * 第二参数可以是 defaultMessage，也可以是插值对象（l(id, { n })）。
 */
export const l = (
  id: string,
  defaultMessage?: string | IntlValues,
  value?: IntlValues,
): string => {
  if (!id) {
    return '';
  }
  const args = resolveIntlArgs(defaultMessage, value);
  return getIntl().formatMessage(
    { id: id as never, defaultMessage: args.defaultMessage },
    args.values as never,
  );
};

/** 与 l 相同，给类组件 / 事件回调用（历史别名）。 */
export const lGet = (
  id: string,
  defaultMessage?: string | IntlValues,
  value?: IntlValues,
): string | JSX.Element => {
  if (!id) {
    return '';
  }
  const args = resolveIntlArgs(defaultMessage, value);
  return getIntl().formatMessage(
    { id: id as never, defaultMessage: args.defaultMessage },
    args.values as never,
  );
};

/** 组件渲染期订阅 locale。必须在函数组件顶层调用，禁止放进 .map() / 分支。 */
export const useL = (
  id: string,
  defaultMessage?: string | IntlValues,
  value?: IntlValues,
): string => {
  const intl = useIntl();
  if (!id) {
    return '';
  }
  const args = resolveIntlArgs(defaultMessage, value);
  return intl.formatMessage(
    { id: id as never, defaultMessage: args.defaultMessage },
    args.values as never,
  );
};
export const lElement = (
  id: string,
  defaultMessage?: string | IntlValues,
  value?: IntlValues,
): string | JSX.Element => {
  if (!id) {
    return '';
  }
  const args = resolveIntlArgs(defaultMessage, value);
  return (
    <FormattedMessage
      id={id}
      defaultMessage={args.defaultMessage}
      values={args.values}
    />
  );
};

export const parseSplitI18nToWaterMarkList = (i18nMsg: string, splitter: string): string[] => {
  return i18nMsg.split(splitter);
};
