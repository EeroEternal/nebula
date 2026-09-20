import cn from 'classnames';
import { FC, PropsWithChildren, ButtonHTMLAttributes } from 'react';

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** 额外 className（兼容旧拼写） */
  classNames?: string;
}

const IconButton: FC<PropsWithChildren<IconButtonProps>> = ({
  className = '',
  classNames = '',
  onClick,
  disabled = false,
  children,
  type = 'button',
  ...rest
}) => {
  return (
    <button
      type={type}
      disabled={disabled}
      className={cn(
        'group text-default font-medium inline-flex items-center justify-center select-none cursor-pointer w-9 h-9 rounded-md hover:bg-background',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-offset-1',
        disabled && 'cursor-not-allowed opacity-50 pointer-events-none',
        className,
        classNames,
      )}
      onClick={onClick}
      {...rest}
    >
      {children}
    </button>
  );
};
export default IconButton;
