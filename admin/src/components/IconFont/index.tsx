import classNames from 'classnames';
import { FC, HTMLAttributes } from 'react';

interface IconFontProps extends HTMLAttributes<HTMLDivElement> {
  name: string;
}
const IconFont: FC<IconFontProps> = ({
  name,
  className,
  style,
  ...reset
}) => {
  return (
    <i
      {...reset}
      style={{ ...style || {} }}
      className={classNames('iconfont cursor-pointer inline-block leading-none', name, className)}
    ></i>
  );
}
export default IconFont;

