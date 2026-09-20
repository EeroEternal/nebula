import classnames from 'classnames';
import { FC, PropsWithChildren } from 'react';
import Markdown from 'react-markdown';
import rehypeRaw from 'rehype-raw';
import remarkGfm from 'remark-gfm';

interface ReactMarkdownProps {
  classnames?: string;
  parseHtml?: boolean;
}
const ReactMarkdown: FC<PropsWithChildren<ReactMarkdownProps>> = ({
  children,
  classnames: classname,
  parseHtml = false,
}) => {
  if (typeof children !== 'string') {
    return <span>{String(children)}</span>;
  }
  return (
    <Markdown
      skipHtml={false}
      remarkPlugins={[remarkGfm]}
      rehypePlugins={parseHtml ? [rehypeRaw] : []}
      className={classnames('markdown-body break-word', classname)}
      components={{
        a: (props) => {
          // eslint-disable-next-line @typescript-eslint/no-unused-vars
          const { node, ...rest } = props;
          return <a {...rest} target="_blank" />;
        },
      }}
    >
      {children}
    </Markdown>
  );
};
export default ReactMarkdown;
